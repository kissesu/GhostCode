/// @file verification.rs
/// @description Ralph 验证状态机
///              实现 Ralph 验证循环的状态机核心逻辑：
///              - 7 种检查类型（Build/Test/Lint/Functionality/Architect/Todo/ErrorFree）
///              - 状态迁移：Running → Approved/Rejected/Cancelled
///              - 最大迭代次数限制（默认 10 轮）
///              - 每轮失败原因保留
///              - 纯函数 transition 便于属性基测试
/// @author Atlas.oi
/// @date 2026-03-03

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ============================================
// 错误类型定义
// ============================================

/// 验证状态机操作错误
#[derive(Debug, Error, PartialEq)]
pub enum VerifyError {
    /// 运行已存在，不可重复启动
    #[error("运行 ({group_id}, {run_id}) 已经存在")]
    RunAlreadyExists { group_id: String, run_id: String },

    /// 运行不存在
    #[error("运行 ({group_id}, {run_id}) 不存在")]
    RunNotFound { group_id: String, run_id: String },

    /// 当前状态下操作非法（如终态下继续推进）
    #[error("操作非法：{reason}")]
    IllegalOperation { reason: String },
}

// ============================================
// 核心数据类型定义
// ============================================

/// 验证检查类型
///
/// Ralph 循环中的 7 种检查维度，涵盖代码质量全景
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VerificationCheckKind {
    /// 构建检查：代码是否能成功编译/构建
    Build,
    /// 测试检查：所有测试是否通过
    Test,
    /// Lint 检查：代码风格和静态分析
    Lint,
    /// 功能检查：功能是否正确实现
    Functionality,
    /// 架构检查：架构设计是否符合规范
    Architect,
    /// Todo 检查：是否存在遗留的 TODO/FIXME
    Todo,
    /// 无错误检查：代码中是否存在错误
    ErrorFree,
}

/// 单项检查状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CheckStatus {
    /// 尚未检查
    Pending,
    /// 检查通过
    Passed,
    /// 检查失败，携带失败原因
    Failed(String),
}

/// 运行整体状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RunStatus {
    /// 运行中（验证循环进行中）
    Running,
    /// 已批准（所有检查通过）
    Approved,
    /// 已拒绝（达到最大迭代次数后仍有失败）
    Rejected,
    /// 已取消
    Cancelled,
}

/// 单项检查结果，记录检查类型和状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerificationCheckResult {
    /// 检查类型
    pub kind: VerificationCheckKind,
    /// 检查状态
    pub status: CheckStatus,
}

/// 单轮迭代记录
///
/// 保留每轮迭代的所有检查结果和失败原因，便于追溯
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerificationIteration {
    /// 本轮所有检查结果
    pub checks: Vec<VerificationCheckResult>,
    /// 本轮失败原因汇总
    pub failure_reasons: Vec<String>,
}

/// 运行状态快照
///
/// 完整描述一次验证运行的当前状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunState {
    /// 运行 ID
    pub run_id: String,
    /// 所属 Group ID
    pub group_id: String,
    /// 当前整体状态
    pub status: RunStatus,
    /// 当前迭代次数（从 0 开始计数）
    pub iteration: u32,
    /// 最大允许迭代次数
    pub max_iterations: u32,
    /// 当前轮次各项检查状态
    pub current_checks: Vec<(VerificationCheckKind, CheckStatus)>,
    /// 历史迭代记录
    pub history: Vec<VerificationIteration>,
}

// ============================================
// 事件定义
// ============================================

/// 验证状态机事件
///
/// 所有能改变状态机状态的操作，以事件形式封装
#[derive(Debug, Clone, PartialEq)]
pub enum VerifyEvent {
    /// 启动新运行
    Start { group_id: String, run_id: String },
    /// 某项检查通过
    CheckPassed(VerificationCheckKind),
    /// 某项检查失败，携带失败原因
    CheckFailed(VerificationCheckKind, String),
    /// 推进到下一迭代（结算本轮结果）
    AdvanceIteration,
    /// 取消当前运行
    Cancel,
}

// ============================================
// 辅助函数
// ============================================

/// 返回所有 7 种检查类型的完整列表
///
/// 用于初始化时创建 Pending 检查列表
fn all_check_kinds() -> Vec<VerificationCheckKind> {
    vec![
        VerificationCheckKind::Build,
        VerificationCheckKind::Test,
        VerificationCheckKind::Lint,
        VerificationCheckKind::Functionality,
        VerificationCheckKind::Architect,
        VerificationCheckKind::Todo,
        VerificationCheckKind::ErrorFree,
    ]
}

/// 创建初始检查列表（所有项均为 Pending）
fn initial_checks() -> Vec<(VerificationCheckKind, CheckStatus)> {
    all_check_kinds()
        .into_iter()
        .map(|k| (k, CheckStatus::Pending))
        .collect()
}

/// 判断 RunState 是否处于终态
///
/// 终态包括：Approved / Rejected / Cancelled
pub fn is_terminal(state: &RunState) -> bool {
    matches!(
        state.status,
        RunStatus::Approved | RunStatus::Rejected | RunStatus::Cancelled
    )
}

// ============================================
// 核心纯函数：状态迁移
// ============================================

/// 纯函数：根据当前状态和事件，计算下一状态
///
/// 业务逻辑说明：
/// 1. 终态下拒绝任何非 Start 事件
/// 2. CheckPassed/CheckFailed 更新当前轮次对应检查状态
/// 3. AdvanceIteration 结算本轮结果：
///    - 全部通过 → Approved
///    - 有失败但未达最大迭代 → iteration+1，重置检查，保留历史
///    - 有失败且达最大迭代 → Rejected
/// 4. Cancel → Cancelled
///
/// @param state - 当前状态
/// @param event - 触发事件
/// @returns 成功返回新状态，失败返回错误
pub fn transition(state: &RunState, event: &VerifyEvent) -> Result<RunState, VerifyError> {
    // ============================================
    // 终态检查：终态下拒绝所有事件
    // ============================================
    if is_terminal(state) {
        return Err(VerifyError::IllegalOperation {
            reason: format!("当前状态 {:?} 为终态，不可接受事件 {:?}", state.status, event),
        });
    }

    let mut new_state = state.clone();

    match event {
        VerifyEvent::Start { .. } => {
            // Start 事件通过 VerificationStateStore::start_run 处理，此处不支持
            return Err(VerifyError::IllegalOperation {
                reason: "Start 事件不通过 transition 处理，请使用 start_run 方法".to_string(),
            });
        }

        VerifyEvent::CheckPassed(kind) => {
            // ============================================
            // 更新指定检查类型为 Passed
            // 如果该检查已经有状态，覆盖更新
            // ============================================
            update_check_status(&mut new_state.current_checks, kind, CheckStatus::Passed);
        }

        VerifyEvent::CheckFailed(kind, reason) => {
            // ============================================
            // 更新指定检查类型为 Failed，携带失败原因
            // ============================================
            update_check_status(
                &mut new_state.current_checks,
                kind,
                CheckStatus::Failed(reason.clone()),
            );
        }

        VerifyEvent::AdvanceIteration => {
            // ============================================
            // 结算本轮结果
            // 1. 收集所有失败原因
            // 2. 保存本轮到历史记录
            // 3. 根据结果决定下一状态
            // ============================================

            // 校验是否所有检查都已完成（非 Pending）
            // 防止在部分检查还是 Pending 状态时提前结算
            let has_pending = new_state
                .current_checks
                .iter()
                .any(|(_, status)| matches!(status, CheckStatus::Pending));

            if has_pending {
                return Err(VerifyError::IllegalOperation {
                    reason: "存在未完成的检查项（Pending），不可结算本轮".to_string(),
                });
            }

            // 收集本轮失败原因
            let failure_reasons: Vec<String> = new_state
                .current_checks
                .iter()
                .filter_map(|(_, status)| {
                    if let CheckStatus::Failed(reason) = status {
                        Some(reason.clone())
                    } else {
                        None
                    }
                })
                .collect();

            // 保存本轮历史记录
            let iteration_record = VerificationIteration {
                checks: new_state
                    .current_checks
                    .iter()
                    .map(|(k, s)| VerificationCheckResult {
                        kind: k.clone(),
                        status: s.clone(),
                    })
                    .collect(),
                failure_reasons: failure_reasons.clone(),
            };
            new_state.history.push(iteration_record);

            if failure_reasons.is_empty() {
                // 本轮全部通过（所有检查都是 Passed） → Approved
                new_state.status = RunStatus::Approved;
            } else {
                // 本轮有失败，检查是否达到最大迭代次数
                new_state.iteration += 1;

                if new_state.iteration >= new_state.max_iterations {
                    // 达到最大迭代次数 → Rejected
                    new_state.status = RunStatus::Rejected;
                } else {
                    // 未达最大迭代次数，重置当前轮次检查，继续运行
                    new_state.current_checks = initial_checks();
                }
            }
        }

        VerifyEvent::Cancel => {
            // ============================================
            // 取消运行，直接进入终态 Cancelled
            // ============================================
            new_state.status = RunStatus::Cancelled;
        }
    }

    Ok(new_state)
}

/// 辅助函数：更新检查状态列表中指定类型的状态
///
/// 如果该类型已存在则更新，不存在则追加
fn update_check_status(
    checks: &mut Vec<(VerificationCheckKind, CheckStatus)>,
    kind: &VerificationCheckKind,
    new_status: CheckStatus,
) {
    if let Some(entry) = checks.iter_mut().find(|(k, _)| k == kind) {
        entry.1 = new_status;
    } else {
        checks.push((kind.clone(), new_status));
    }
}

// ============================================
// VerificationStateStore：状态存储器
// ============================================

/// 验证状态存储
///
/// 内部维护 HashMap<(group_id, run_id), RunState>
/// 负责运行的生命周期管理和事件应用
pub struct VerificationStateStore {
    /// 所有活跃和历史运行的状态存储
    /// key: (group_id, run_id)，value: RunState
    states: HashMap<(String, String), RunState>,
}

impl VerificationStateStore {
    /// 创建新的验证状态存储实例
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
        }
    }

    /// 启动新的验证运行
    ///
    /// 创建初始 Running 状态，所有 7 项检查设为 Pending
    ///
    /// @param group_id - 所属 Group ID
    /// @param run_id - 运行 ID
    /// @returns 成功返回 ()，如果已存在则返回错误
    pub fn start_run(&mut self, group_id: String, run_id: String) -> Result<(), VerifyError> {
        let key = (group_id.clone(), run_id.clone());

        // 检查是否已存在同一 (group_id, run_id) 的运行
        if self.states.contains_key(&key) {
            return Err(VerifyError::RunAlreadyExists { group_id, run_id });
        }

        let state = RunState {
            run_id: run_id.clone(),
            group_id: group_id.clone(),
            status: RunStatus::Running,
            iteration: 0,
            max_iterations: 10,
            current_checks: initial_checks(),
            history: vec![],
        };

        self.states.insert(key, state);
        Ok(())
    }

    /// 获取指定运行的当前状态（不可变引用）
    ///
    /// @param group_id - 所属 Group ID
    /// @param run_id - 运行 ID
    /// @returns 存在则返回 Some(&RunState)，否则返回 None
    pub fn get_run(&self, group_id: &str, run_id: &str) -> Option<&RunState> {
        self.states.get(&(group_id.to_string(), run_id.to_string()))
    }

    /// 对指定运行应用事件，更新状态
    ///
    /// 内部调用纯函数 transition 计算新状态并持久化
    ///
    /// @param group_id - 所属 Group ID
    /// @param run_id - 运行 ID
    /// @param event - 要应用的事件
    /// @returns 成功返回 ()，失败返回错误
    pub fn apply_event(
        &mut self,
        group_id: &str,
        run_id: &str,
        event: VerifyEvent,
    ) -> Result<(), VerifyError> {
        let key = (group_id.to_string(), run_id.to_string());

        // 获取当前状态，不存在则返回错误
        let current_state = self.states.get(&key).ok_or_else(|| VerifyError::RunNotFound {
            group_id: group_id.to_string(),
            run_id: run_id.to_string(),
        })?;

        // 使用纯函数计算新状态
        let new_state = transition(current_state, &event)?;

        // 持久化新状态
        self.states.insert(key, new_state);

        Ok(())
    }
}

impl Default for VerificationStateStore {
    fn default() -> Self {
        Self::new()
    }
}
