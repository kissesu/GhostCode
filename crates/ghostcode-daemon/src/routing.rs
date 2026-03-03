//! 路由状态管理模块
//!
//! 维护正在执行的路由任务的状态，包括任务注册、状态更新、查询和取消。
//! 同时持有 SovereigntyGuard，提供代码主权检查能力。
//!
//! @author Atlas.oi
//! @date 2026-03-02

use std::collections::HashMap;
use tokio::sync::RwLock;

use ghostcode_router::sovereignty::SovereigntyGuard;

// ============================================
// 数据结构定义
// ============================================

/// 路由任务状态快照
///
/// 记录单个路由任务的完整状态信息，用于查询和日志
#[derive(Debug, Clone, serde::Serialize)]
pub struct RouteTaskState {
    /// 任务唯一标识符（UUID v4）
    pub task_id: String,
    /// 所属 Group ID（用于隔离不同 Group 的任务，防止跨 Group 越权访问）
    pub group_id: String,
    /// 任务当前状态：pending | running | completed | failed | cancelled
    pub status: String,
    /// 目标后端名称（如 "claude" / "codex" / "gemini"）
    pub backend: String,
    /// 任务执行结果（完成时填充输出文本）
    pub result: Option<String>,
}

// ============================================
// 路由状态管理器
// ============================================

/// 路由状态管理器
///
/// 线程安全地维护所有路由任务的状态表，
/// 并持有代码主权守卫（SovereigntyGuard）供 dispatch 调用权限检查。
/// 状态表使用 (group_id, task_id) 组合键，确保不同 Group 间的任务严格隔离。
pub struct RoutingState {
    /// 正在执行的任务表 (group_id, task_id) -> RouteTaskState
    /// 使用复合键确保跨 Group 隔离，防止越权访问
    /// 使用 RwLock 保证多连接并发安全
    tasks: RwLock<HashMap<(String, String), RouteTaskState>>,
    /// 代码主权守卫（默认写入者为 "claude"）
    pub sovereignty: SovereigntyGuard,
}

impl RoutingState {
    /// 创建新的路由状态管理器
    ///
    /// 默认使用 claude 作为写入者，符合代码主权核心规则
    pub fn new() -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
            sovereignty: SovereigntyGuard::new(),
        }
    }

    /// 注册新路由任务
    ///
    /// 以 pending 状态将任务加入状态表。
    /// 调用者负责生成唯一 task_id（通常为 UUID v4）。
    /// 使用 (group_id, task_id) 复合键确保跨 Group 隔离。
    ///
    /// @param group_id - 所属 Group ID
    /// @param task_id - 任务唯一标识符
    /// @param backend - 目标后端名称
    /// @returns 注册后的任务状态快照
    pub async fn register_task(&self, group_id: &str, task_id: &str, backend: &str) -> RouteTaskState {
        let state = RouteTaskState {
            task_id: task_id.to_string(),
            group_id: group_id.to_string(),
            status: "pending".to_string(),
            backend: backend.to_string(),
            result: None,
        };
        let mut tasks = self.tasks.write().await;
        let key = (group_id.to_string(), task_id.to_string());
        tasks.insert(key, state.clone());
        state
    }

    /// 更新任务状态
    ///
    /// 若任务不存在则静默忽略（幂等操作）。
    /// 必须同时提供 group_id 和 task_id，防止跨 Group 操作。
    ///
    /// @param group_id - 所属 Group ID
    /// @param task_id - 目标任务 ID
    /// @param status - 新状态字符串
    /// @param result - 可选的执行结果文本
    pub async fn update_task(&self, group_id: &str, task_id: &str, status: &str, result: Option<String>) {
        let mut tasks = self.tasks.write().await;
        let key = (group_id.to_string(), task_id.to_string());
        if let Some(entry) = tasks.get_mut(&key) {
            entry.status = status.to_string();
            entry.result = result;
        }
    }

    /// 查询单个任务状态
    ///
    /// 必须同时提供 group_id 和 task_id，确保只能访问本 Group 的任务。
    ///
    /// @param group_id - 所属 Group ID
    /// @param task_id - 目标任务 ID
    /// @returns 存在时返回状态快照，否则返回 None
    pub async fn get_task(&self, group_id: &str, task_id: &str) -> Option<RouteTaskState> {
        let tasks = self.tasks.read().await;
        let key = (group_id.to_string(), task_id.to_string());
        tasks.get(&key).cloned()
    }

    /// 取消任务（标记为 cancelled）
    ///
    /// 幂等操作：任务不存在时返回 false，已取消则直接返回 true。
    /// 必须同时匹配 group_id 和 task_id，防止跨 Group 取消。
    ///
    /// @param group_id - 所属 Group ID
    /// @param task_id - 目标任务 ID
    /// @returns true 表示成功取消（或已取消），false 表示任务不存在
    pub async fn cancel_task(&self, group_id: &str, task_id: &str) -> bool {
        let mut tasks = self.tasks.write().await;
        let key = (group_id.to_string(), task_id.to_string());
        if let Some(entry) = tasks.get_mut(&key) {
            entry.status = "cancelled".to_string();
            true
        } else {
            false
        }
    }

    /// 列出所有已注册的任务
    ///
    /// @returns 所有任务状态快照的列表（顺序不定）
    pub async fn list_tasks(&self) -> Vec<RouteTaskState> {
        let tasks = self.tasks.read().await;
        tasks.values().cloned().collect()
    }
}

impl Default for RoutingState {
    fn default() -> Self {
        Self::new()
    }
}
