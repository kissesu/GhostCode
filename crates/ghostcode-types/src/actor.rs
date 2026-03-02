//! Actor 相关类型定义
//!
//! ActorRole、RuntimeKind、ActorInfo
//! 描述 Agent（Actor）的角色、运行时和状态
//!
//! @author Atlas.oi
//! @date 2026-02-28

use serde::{Deserialize, Serialize};

/// Actor 角色
///
/// - Foreman: 主管 Agent，负责协调和分配任务
/// - Peer: 普通 Agent，执行具体任务
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorRole {
    Foreman,
    Peer,
}

/// Actor 运行时类型
///
/// 标识 Agent 使用的底层 AI 运行时
/// - Claude: Claude Code CLI
/// - Codex: OpenAI Codex CLI
/// - Gemini: Google Gemini CLI
/// - Custom: 自定义运行时（扩展用）
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    Claude,
    Codex,
    Gemini,
    Custom(String),
}

/// Actor 信息
///
/// 描述一个已注册 Actor 的完整状态
///
/// 字段说明：
/// - actor_id: Actor 唯一标识
/// - display_name: 显示名称
/// - role: 角色（Foreman 或 Peer）
/// - runtime: 运行时类型
/// - running: 是否正在运行
/// - pid: 操作系统进程 ID（如果正在运行）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActorInfo {
    /// Actor 唯一标识
    pub actor_id: String,
    /// 显示名称
    pub display_name: String,
    /// 角色
    pub role: ActorRole,
    /// 运行时类型
    pub runtime: RuntimeKind,
    /// 是否正在运行
    pub running: bool,
    /// 操作系统进程 ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
}
