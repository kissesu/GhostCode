// @file dag.rs
// @description DAG（有向无环图）拓扑排序模块
//              用于多模型并行任务调度，将任务集合按依赖层次分组
//              每层内的任务可并行执行，层间串行
// @author Atlas.oi
// @date 2026-03-02

// 参考: ccg-workflow/codeagent-wrapper/executor.go:287-351 - 任务调度拓扑排序实现

use std::collections::{HashMap, VecDeque};
use thiserror::Error;

// ============================================
// 公开数据类型定义
// ============================================

/// 任务节点，代表调度图中的一个可执行任务
#[derive(Debug, Clone)]
pub struct TaskNode {
    /// 任务唯一标识符
    pub id: String,
    /// 该任务依赖的其他任务 ID 列表（这些任务必须先完成）
    pub dependencies: Vec<String>,
}

/// DAG 拓扑排序错误类型
#[derive(Error, Debug)]
pub enum DagError {
    /// 检测到循环依赖，无法完成拓扑排序
    #[error("检测到循环依赖: {0:?}")]
    CycleDetected(Vec<String>),

    /// 某任务依赖一个不存在于任务集合中的任务
    #[error("任务 {task} 依赖不存在的任务 {missing}")]
    MissingDependency { task: String, missing: String },

    /// 检测到重复的任务 ID
    #[error("检测到重复的任务 ID: {0}")]
    DuplicateId(String),
}

// ============================================
// 公开 API
// ============================================

/// BFS 拓扑排序，返回按执行层次分组的任务数组
///
/// 业务逻辑：
/// 1. 构建入度表（统计每个节点有多少前驱节点）和邻接表（每个节点的后继节点）
/// 2. 验证所有依赖项均存在于任务集合中，发现缺失则立即报错
/// 3. 将入度为 0 的节点加入队列（第一批可执行任务）
/// 4. BFS 层序遍历：将当前队列中所有节点归为同一层，
///    处理完成后减少后继节点的入度，入度归零则加入下一层队列
/// 5. 环检测：处理完成后若仍有节点未被处理，说明存在循环依赖
///
/// 参考: ccg-workflow/codeagent-wrapper/executor.go:287-351
///
/// @param tasks - 任务节点列表，每个节点包含 ID 和依赖列表
/// @returns Ok(Vec<Vec<TaskNode>>) - 按层分组的任务，同层可并行执行
/// @returns Err(DagError) - 循环依赖或缺失依赖时返回错误
pub fn topological_sort(tasks: Vec<TaskNode>) -> Result<Vec<Vec<TaskNode>>, DagError> {
    // 空输入直接返回空结果
    if tasks.is_empty() {
        return Ok(vec![]);
    }

    // ============================================
    // 第一步：建立索引映射，方便通过 ID 查找节点
    // ============================================

    // id -> 在 tasks 数组中的下标索引（用于最终重组输出）
    // 先检测重复 ID：HashMap::insert 返回 Some 时说明同一 key 被覆盖
    let mut id_to_idx: HashMap<String, usize> = HashMap::with_capacity(tasks.len());
    for (i, t) in tasks.iter().enumerate() {
        if id_to_idx.insert(t.id.clone(), i).is_some() {
            return Err(DagError::DuplicateId(t.id.clone()));
        }
    }

    // ============================================
    // 第二步：检查缺失依赖
    // 所有 dependencies 中引用的 ID 必须存在于任务集合中
    // ============================================
    for task in &tasks {
        for dep_id in &task.dependencies {
            if !id_to_idx.contains_key(dep_id) {
                return Err(DagError::MissingDependency {
                    task: task.id.clone(),
                    missing: dep_id.clone(),
                });
            }
        }
    }

    // ============================================
    // 第三步：构建入度表和邻接表
    // - in_degree[i]: 节点 i 的入度（有多少节点的 dependencies 包含该节点）
    // - adjacency[i]: 节点 i 的后继节点下标列表（该节点完成后需要通知的节点）
    // ============================================
    let n = tasks.len();

    // 初始化入度表为 0
    let mut in_degree = vec![0usize; n];

    // 邻接表：节点 i 完成后，需要减少 in_degree 的后继节点列表
    let mut adjacency: Vec<Vec<usize>> = vec![vec![]; n];

    for (i, task) in tasks.iter().enumerate() {
        for dep_id in &task.dependencies {
            // dep_idx 是 task 的依赖节点下标
            let dep_idx = id_to_idx[dep_id];
            // dep_idx -> i：dep_idx 完成后，i 的入度减 1
            adjacency[dep_idx].push(i);
            // 节点 i 的入度加 1
            in_degree[i] += 1;
        }
    }

    // ============================================
    // 第四步：BFS 层序遍历
    // 初始队列：所有入度为 0 的节点（无依赖，可立即执行）
    // ============================================

    // 当前层的节点下标队列
    let mut queue: VecDeque<usize> = in_degree
        .iter()
        .enumerate()
        .filter(|(_, &deg)| deg == 0)
        .map(|(i, _)| i)
        .collect();

    // 按层收集结果（存节点下标，最后统一转换为 TaskNode）
    let mut result_layers: Vec<Vec<usize>> = vec![];

    // 已处理的节点总数（用于环检测）
    let mut processed_count = 0usize;

    // 逐层 BFS
    while !queue.is_empty() {
        // 当前层的所有节点下标
        let current_layer: Vec<usize> = queue.drain(..).collect();
        processed_count += current_layer.len();

        // 处理当前层的每个节点，更新其后继节点的入度
        for &node_idx in &current_layer {
            for &successor_idx in &adjacency[node_idx] {
                in_degree[successor_idx] -= 1;
                // 入度归零：该后继节点的所有依赖均已完成，加入下一层
                if in_degree[successor_idx] == 0 {
                    queue.push_back(successor_idx);
                }
            }
        }

        result_layers.push(current_layer);
    }

    // ============================================
    // 第五步：环检测
    // 若已处理节点数 < 总节点数，说明有节点因入度始终 > 0 而未被处理
    // 这意味着这些节点形成了循环依赖
    // ============================================
    if processed_count != n {
        // 找出未被处理的节点，作为环的一部分返回
        let id_to_layer: HashMap<usize, usize> = result_layers
            .iter()
            .enumerate()
            .flat_map(|(layer_idx, layer)| layer.iter().map(move |&idx| (idx, layer_idx)))
            .collect();

        let cycle_nodes: Vec<String> = (0..n)
            .filter(|idx| !id_to_layer.contains_key(idx))
            .map(|idx| tasks[idx].id.clone())
            .collect();

        return Err(DagError::CycleDetected(cycle_nodes));
    }

    // ============================================
    // 第六步：将下标层转换为 TaskNode 层
    // 使用 tasks 的所有权，通过下标取出对应节点
    // ============================================

    // 将 tasks 转换为 Option<TaskNode> 数组，方便按索引取出所有权
    let mut tasks_opt: Vec<Option<TaskNode>> = tasks.into_iter().map(Some).collect();

    let layers: Vec<Vec<TaskNode>> = result_layers
        .into_iter()
        .map(|layer_indices| {
            layer_indices
                .into_iter()
                .map(|idx| tasks_opt[idx].take().expect("节点不应被重复取出"))
                .collect()
        })
        .collect();

    Ok(layers)
}
