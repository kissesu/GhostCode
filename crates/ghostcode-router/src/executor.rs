// @file executor.rs
// @description 并行执行引擎，基于 DAG 拓扑排序实现层间串行+层内并行的任务调度
//              使用 Semaphore 限流，支持依赖失败传播和取消令牌
//              参考: ccg-workflow/codeagent-wrapper/executor.go:353-515
// @author Atlas.oi
// @date 2026-03-02

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Semaphore, SemaphorePermit};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::dag::{TaskNode, topological_sort};
use crate::process::{ProcessManager, ProcessOutput, ProcessError};

// ============================================================
// 公开类型定义
// ============================================================

/// 可执行任务规格，包含所有调度和执行所需信息
#[derive(Debug, Clone)]
pub struct ExecutableTask {
    /// 任务唯一标识符
    pub id: String,
    /// 可执行命令名称
    pub command: String,
    /// 命令行参数列表
    pub args: Vec<String>,
    /// 可选的 stdin 数据（None 表示不提供 stdin）
    pub stdin_data: Option<String>,
    /// 依赖的任务 ID 列表（这些任务必须先成功完成）
    pub dependencies: Vec<String>,
    /// 任务超时时间
    pub timeout: Duration,
}

/// 任务执行状态
#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    /// 任务成功完成
    Success,
    /// 任务失败（含错误信息）
    Failed(String),
    /// 任务被跳过（含跳过原因）
    Skipped(String),
    /// 任务被取消
    Cancelled,
}

/// 单个任务的执行结果
#[derive(Debug)]
pub struct TaskResult {
    /// 任务 ID
    pub id: String,
    /// 执行状态
    pub status: TaskStatus,
    /// 进程输出（成功时有值，失败/跳过/取消时可能为 None）
    pub output: Option<ProcessOutput>,
    /// 实际执行时长
    pub duration: Duration,
}

/// 执行引擎配置
pub struct ExecutorConfig {
    /// 最大并行工作数（0 表示无限制，实际上限 100）
    pub max_workers: usize,
    /// 取消令牌，触发后停止所有正在执行的任务
    pub cancel: CancellationToken,
}

/// 并行执行引擎
///
/// 业务逻辑：
/// 1. 使用 DAG 拓扑排序将任务分层（层间串行，层内并行）
/// 2. Semaphore 限制同时执行的任务数量
/// 3. 依赖失败传播：任意依赖失败则下游任务标记为 Skipped
/// 4. 取消传播：cancel token 触发时停止所有正在执行的任务
pub struct Executor;

impl Executor {
    /// 执行 DAG 任务集
    ///
    /// 业务逻辑：
    /// 1. 将 ExecutableTask 转换为 TaskNode 传给 topological_sort 分层
    /// 2. 逐层执行：层间串行，层内并行（tokio::spawn + JoinSet）
    /// 3. Semaphore 限流：acquire permit 后才执行，释放 permit 后其他任务可以开始
    /// 4. 依赖失败传播：检查所有依赖状态，任意失败则标记 Skipped
    /// 5. 取消传播：cancel token 触发时所有正在执行的任务收到子取消令牌
    ///
    /// @param tasks - 可执行任务列表，包含依赖关系
    /// @param config - 执行配置（最大并行数 + 取消令牌）
    /// @returns 所有任务的执行结果列表
    pub async fn run(
        tasks: Vec<ExecutableTask>,
        config: ExecutorConfig,
    ) -> Vec<TaskResult> {
        // 空输入直接返回空结果
        if tasks.is_empty() {
            return vec![];
        }

        // ============================================
        // 第一步：将 ExecutableTask 转为 TaskNode，进行 DAG 拓扑排序
        // 获取按层分组的执行顺序
        // ============================================
        let task_map: HashMap<String, ExecutableTask> = tasks
            .into_iter()
            .map(|t| (t.id.clone(), t))
            .collect();

        let task_nodes: Vec<TaskNode> = task_map
            .values()
            .map(|t| TaskNode {
                id: t.id.clone(),
                dependencies: t.dependencies.clone(),
            })
            .collect();

        let layers = match topological_sort(task_nodes) {
            Ok(layers) => layers,
            Err(e) => {
                // DAG 错误（循环依赖或缺失依赖）：所有任务标记为失败
                return task_map
                    .keys()
                    .map(|id| TaskResult {
                        id: id.clone(),
                        status: TaskStatus::Failed(format!("DAG 错误: {}", e)),
                        output: None,
                        duration: Duration::ZERO,
                    })
                    .collect();
            }
        };

        // ============================================
        // 第二步：初始化 Semaphore 限制并发数
        // max_workers=0 时视为无限制（设为 100），否则 clamp 到 [1, 100]
        // ============================================
        let worker_limit = if config.max_workers == 0 {
            100
        } else {
            config.max_workers.clamp(1, 100)
        };
        let semaphore = Arc::new(Semaphore::new(worker_limit));

        // ============================================
        // 第三步：维护已完成任务的状态表
        // 用于依赖失败传播检查
        // ============================================

        // 已完成任务的状态记录（id -> TaskStatus）
        let mut completed: HashMap<String, TaskStatus> = HashMap::new();
        // 最终结果收集
        let mut all_results: Vec<TaskResult> = Vec::with_capacity(task_map.len());

        // ============================================
        // 第四步：逐层执行
        // 层间串行保证依赖关系，层内并行提高吞吐
        // ============================================
        for layer in layers {
            // 检查取消令牌：若已取消则跳过后续所有层
            if config.cancel.is_cancelled() {
                for node in &layer {
                    let start = Instant::now();
                    all_results.push(TaskResult {
                        id: node.id.clone(),
                        status: TaskStatus::Cancelled,
                        output: None,
                        duration: start.elapsed(),
                    });
                }
                continue;
            }

            // ============================================
            // 层内并行执行：每个任务 spawn 独立 tokio task
            // ============================================
            let mut join_set: JoinSet<TaskResult> = JoinSet::new();

            for node in layer {
                let task_id = node.id.clone();

                // ============================================
                // 依赖失败传播检查
                // 如果任意依赖不是 Success，则跳过该任务
                // ============================================
                let skip_reason = node.dependencies.iter().find_map(|dep_id| {
                    match completed.get(dep_id) {
                        Some(TaskStatus::Success) => None,
                        Some(TaskStatus::Failed(msg)) => {
                            Some(format!("依赖任务 {} 失败: {}", dep_id, msg))
                        }
                        Some(TaskStatus::Skipped(reason)) => {
                            Some(format!("依赖任务 {} 被跳过: {}", dep_id, reason))
                        }
                        Some(TaskStatus::Cancelled) => {
                            Some(format!("依赖任务 {} 被取消", dep_id))
                        }
                        // 依赖不在 completed 中（理论上不应发生，DAG 排序保证层序）
                        None => Some(format!("依赖任务 {} 结果未知", dep_id)),
                    }
                });

                if let Some(reason) = skip_reason {
                    // 标记为跳过，不需要执行
                    join_set.spawn(async move {
                        TaskResult {
                            id: task_id,
                            status: TaskStatus::Skipped(reason),
                            output: None,
                            duration: Duration::ZERO,
                        }
                    });
                    continue;
                }

                // ============================================
                // 获取 Semaphore permit（限制并发数）
                // 取得 permit 后才真正开始执行
                // ============================================
                let semaphore_clone = Arc::clone(&semaphore);
                let cancel_token = config.cancel.clone();

                // 从 task_map 中取出任务数据用于 spawn
                let task = match task_map.get(&task_id) {
                    Some(t) => t.clone(),
                    None => {
                        // 不应发生（DAG 中的节点都来自 task_map）
                        join_set.spawn(async move {
                            TaskResult {
                                id: task_id,
                                status: TaskStatus::Failed("任务数据丢失（内部错误）".to_string()),
                                output: None,
                                duration: Duration::ZERO,
                            }
                        });
                        continue;
                    }
                };

                join_set.spawn(async move {
                    let start = Instant::now();

                    // 在取得 permit 之前先检查取消状态
                    if cancel_token.is_cancelled() {
                        return TaskResult {
                            id: task.id.clone(),
                            status: TaskStatus::Cancelled,
                            output: None,
                            duration: start.elapsed(),
                        };
                    }

                    // 获取 Semaphore permit（阻塞直到有空位）
                    // permit 在 task 执行完毕后 drop 自动释放
                    let _permit: SemaphorePermit = match semaphore_clone.acquire().await {
                        Ok(p) => p,
                        Err(_) => {
                            // Semaphore 被关闭（不应发生）
                            return TaskResult {
                                id: task.id.clone(),
                                status: TaskStatus::Failed("Semaphore 被关闭".to_string()),
                                output: None,
                                duration: start.elapsed(),
                            };
                        }
                    };

                    // 再次检查取消状态（在等待 permit 期间可能已取消）
                    if cancel_token.is_cancelled() {
                        return TaskResult {
                            id: task.id.clone(),
                            status: TaskStatus::Cancelled,
                            output: None,
                            duration: start.elapsed(),
                        };
                    }

                    // ============================================
                    // 执行任务：调用 ProcessManager::run_command
                    // 将 args Vec<String> 转为 &[&str] 切片
                    // ============================================
                    let args_refs: Vec<&str> = task.args.iter().map(|s| s.as_str()).collect();
                    let stdin_ref = task.stdin_data.as_deref();

                    // 为每个任务创建子取消令牌，方便单独取消
                    let child_cancel = cancel_token.child_token();

                    let result = ProcessManager::run_command(
                        &task.command,
                        &args_refs,
                        stdin_ref,
                        task.timeout,
                        child_cancel,
                    )
                    .await;

                    let duration = start.elapsed();

                    match result {
                        Ok(output) => TaskResult {
                            id: task.id.clone(),
                            status: TaskStatus::Success,
                            output: Some(output),
                            duration,
                        },
                        Err(ProcessError::Cancelled) => TaskResult {
                            id: task.id.clone(),
                            status: TaskStatus::Cancelled,
                            output: None,
                            duration,
                        },
                        Err(e) => TaskResult {
                            id: task.id.clone(),
                            status: TaskStatus::Failed(e.to_string()),
                            output: None,
                            duration,
                        },
                    }
                });
            }

            // ============================================
            // 等待当前层所有任务完成，收集结果
            // 更新 completed 状态表供下层依赖检查使用
            // ============================================
            while let Some(result) = join_set.join_next().await {
                match result {
                    Ok(task_result) => {
                        // 记录到状态表（供下层依赖传播检查）
                        completed.insert(task_result.id.clone(), task_result.status.clone());
                        all_results.push(task_result);
                    }
                    Err(e) => {
                        // tokio task 本身发生 panic（不应发生）
                        all_results.push(TaskResult {
                            id: "unknown".to_string(),
                            status: TaskStatus::Failed(format!("任务 panic: {}", e)),
                            output: None,
                            duration: Duration::ZERO,
                        });
                    }
                }
            }
        }

        all_results
    }
}
