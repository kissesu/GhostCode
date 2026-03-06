//! Dashboard DTO 类型定义
//!
//! 供 Web Dashboard 查询接口使用的数据传输对象
//! 从账本事件投影构建，只读视图
//!
//! @author Atlas.oi
//! @date 2026-03-03

use serde::{Deserialize, Serialize};

/// 账本时间线条目（供 Dashboard 展示）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LedgerTimelineItem {
    /// 事件唯一标识
    pub id: String,
    /// ISO 8601 UTC 时间戳
    pub ts: String,
    /// 事件类型字符串（序列化后的 EventKind）
    pub kind: String,
    /// 所属 Group ID
    pub group_id: String,
    /// 触发者
    pub by: String,
    /// 事件负载摘要（截断后的 JSON 字符串，最多 200 字符）
    pub data_summary: String,
}

/// Agent 状态视图（供 Dashboard 展示）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentStatusView {
    /// Actor ID
    pub actor_id: String,
    /// Runtime 类型（"claude"/"codex"/"gemini"/"custom"）
    pub runtime: String,
    /// 最后已知状态（"active"/"stopped"/"unknown"）
    pub status: String,
    /// 最后活动时间戳
    pub last_seen: Option<String>,
    /// Agent 显示名称（从 ActorStart 事件的 display_name 字段取值，如 "Code Reviewer"）
    pub display_name: Option<String>,
    /// Agent 类型标识（原始值，如 "feature-dev:code-reviewer"）
    pub agent_type: Option<String>,
}

/// Dashboard 快照（聚合视图）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSnapshot {
    /// Group ID
    pub group_id: String,
    /// 快照生成时间戳
    pub snapshot_ts: String,
    /// 事件总数
    pub total_events: u64,
    /// 活跃 Agent 列表
    pub agents: Vec<AgentStatusView>,
    /// 最近 N 条时间线（默认 20 条）
    pub recent_timeline: Vec<LedgerTimelineItem>,
}

/// 时间线分页查询结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelinePage {
    /// 本页条目
    pub items: Vec<LedgerTimelineItem>,
    /// 下一页游标（None 表示已到末尾）
    pub next_cursor: Option<String>,
    /// 总事件数
    pub total: u64,
}
