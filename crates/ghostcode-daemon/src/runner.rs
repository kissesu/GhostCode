//! Headless Runner - Actor 运行时状态机
//!
//! 管理 Headless Actor 的内存运行时状态，包括：
//! - HeadlessSession: 运行中的 Actor 会话（内存态）
//! - HeadlessState: 会话快照（用于 API 响应序列化）
//! - HeadlessStatus: 状态枚举（Idle/Working/Waiting/Stopped）
//! - LifecycleError: 生命周期操作错误类型
//!
//! 参考: cccc/src/cccc/runners/headless.py - Headless session 状态机
//!
//! @author Atlas.oi
//! @date 2026-03-01

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Headless Actor 状态枚举
///
/// 对应 headless runner 的四种运行状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadlessStatus {
    /// 空闲，等待任务
    Idle,
    /// 正在处理任务
    Working,
    /// 等待外部输入/确认
    Waiting,
    /// 已停止
    Stopped,
}

/// Headless Session（内存中的运行时状态）
///
/// 生命周期与 Actor 的 start/stop 操作绑定：
/// - start_actor 时创建并插入全局 sessions 表
/// - stop_actor 时从 sessions 表移除
/// - 不持久化到磁盘，重启后通过 restore_running_actors 恢复
pub struct HeadlessSession {
    /// 所属 Group ID
    pub group_id: String,
    /// Actor ID
    pub actor_id: String,
    /// 当前运行状态
    pub status: HeadlessStatus,
    /// 当前处理的任务 ID（Working 状态时有值）
    pub current_task_id: Option<String>,
    /// 最后处理的消息 ID（用于幂等性判断）
    pub last_message_id: Option<String>,
    /// Session 启动时间，ISO 8601 格式
    pub started_at: String,
    /// 最后更新时间，ISO 8601 格式（用于超时检测）
    pub updated_at: String,
}

impl HeadlessSession {
    /// 创建新的 Headless Session
    ///
    /// 初始状态为 Idle，started_at 和 updated_at 均设为当前时间
    ///
    /// @param group_id - 所属 Group ID
    /// @param actor_id - Actor ID
    pub fn new(group_id: impl Into<String>, actor_id: impl Into<String>) -> Self {
        let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
        Self {
            group_id: group_id.into(),
            actor_id: actor_id.into(),
            status: HeadlessStatus::Idle,
            current_task_id: None,
            last_message_id: None,
            started_at: now.clone(),
            updated_at: now,
        }
    }

    /// 更新 Session 状态
    ///
    /// 同步更新 status、current_task_id 和 updated_at
    ///
    /// @param status - 新状态
    /// @param task_id - 关联的任务 ID（Working 时提供，其他状态可为 None）
    pub fn set_status(&mut self, status: HeadlessStatus, task_id: Option<String>) {
        self.status = status;
        self.current_task_id = task_id;
        self.updated_at = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    }

    /// 记录最后处理的消息 ID
    ///
    /// 用于消息幂等性判断，同时刷新 updated_at
    ///
    /// @param message_id - 消息 ID
    pub fn set_last_message(&mut self, message_id: String) {
        self.last_message_id = Some(message_id);
        self.updated_at = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    }

    /// 检测 Session 是否超时
    ///
    /// 基于 updated_at 和当前时间计算经过秒数
    /// 解析失败时安全返回 false，不中断正常流程
    ///
    /// @param timeout_secs - 超时阈值（秒）
    /// @return 是否超时
    pub fn is_timed_out(&self, timeout_secs: u64) -> bool {
        if let Ok(updated) = DateTime::parse_from_rfc3339(&self.updated_at) {
            let elapsed = Utc::now().signed_duration_since(updated);
            elapsed.num_seconds() as u64 > timeout_secs
        } else {
            false
        }
    }

    /// 转换为 HeadlessState 快照
    ///
    /// 生成用于 API 响应的不可变快照，包含所有字段的克隆
    pub fn to_state(&self) -> HeadlessState {
        HeadlessState {
            v: 1,
            group_id: self.group_id.clone(),
            actor_id: self.actor_id.clone(),
            status: self.status.clone(),
            current_task_id: self.current_task_id.clone(),
            last_message_id: self.last_message_id.clone(),
            started_at: self.started_at.clone(),
            updated_at: self.updated_at.clone(),
        }
    }
}

/// Headless 状态快照（用于 API 响应序列化）
///
/// 不可变快照，由 HeadlessSession::to_state() 生成
/// 用于 headless_status 等 API 的响应数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadlessState {
    /// 协议版本号，固定为 1
    pub v: u8,
    /// 所属 Group ID
    pub group_id: String,
    /// Actor ID
    pub actor_id: String,
    /// 当前运行状态
    pub status: HeadlessStatus,
    /// 当前处理的任务 ID
    pub current_task_id: Option<String>,
    /// 最后处理的消息 ID
    pub last_message_id: Option<String>,
    /// Session 启动时间，ISO 8601 格式
    pub started_at: String,
    /// 最后更新时间，ISO 8601 格式
    pub updated_at: String,
}

/// 生命周期操作错误类型
///
/// 覆盖 start_actor/stop_actor/get_headless_status 等操作可能的错误情况
#[derive(Debug, thiserror::Error)]
pub enum LifecycleError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML 错误: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("账本错误: {0}")]
    Ledger(#[from] ghostcode_ledger::LedgerError),

    #[error("Actor 不存在: group={group_id}, actor={actor_id}")]
    ActorNotFound { group_id: String, actor_id: String },

    #[error("Group 不存在: {0}")]
    GroupNotFound(String),

    #[error("Session 不存在: group={group_id}, actor={actor_id}")]
    SessionNotFound { group_id: String, actor_id: String },

    #[error("Session 已存在: group={group_id}, actor={actor_id}")]
    SessionAlreadyExists { group_id: String, actor_id: String },
}
