//! 事件相关类型定义
//!
//! EventKind（21 种事件类型）和 Event 结构体
//! 参考: cccc/src/cccc/contracts/v1/event.py - 24 种 EventKind（Phase 1 使用 14 种子集）
//!
//! @author Atlas.oi
//! @date 2026-02-28

use serde::{Deserialize, Serialize};

/// 事件类型枚举（Phase 1 子集：21 种）
///
/// 序列化格式为点分隔的 snake_case，例如 GroupCreate -> "group.create"
/// 参考: cccc/src/cccc/contracts/v1/event.py EventKind 枚举
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventKind {
    // === Group 生命周期 ===
    #[serde(rename = "group.create")]
    GroupCreate,
    #[serde(rename = "group.update")]
    GroupUpdate,
    #[serde(rename = "group.start")]
    GroupStart,
    #[serde(rename = "group.stop")]
    GroupStop,
    #[serde(rename = "group.set_state")]
    GroupSetState,

    // === Actor 生命周期 ===
    #[serde(rename = "actor.add")]
    ActorAdd,
    #[serde(rename = "actor.update")]
    ActorUpdate,
    #[serde(rename = "actor.start")]
    ActorStart,
    #[serde(rename = "actor.stop")]
    ActorStop,
    #[serde(rename = "actor.remove")]
    ActorRemove,

    // === 消息 ===
    #[serde(rename = "chat.message")]
    ChatMessage,
    #[serde(rename = "chat.read")]
    ChatRead,
    #[serde(rename = "chat.ack")]
    ChatAck,

    // === 系统 ===
    #[serde(rename = "system.notify")]
    SystemNotify,

    // === Skill Learning ===
    #[serde(rename = "skill.learned")]
    SkillLearned,
    #[serde(rename = "skill.promoted")]
    SkillPromoted,
    #[serde(rename = "skill.rejected")]
    SkillRejected,

    // === Route（多模型调用生命周期） ===
    #[serde(rename = "route.start")]
    RouteStart,
    #[serde(rename = "route.complete")]
    RouteComplete,
    #[serde(rename = "route.error")]
    RouteError,

    // === Dashboard ===
    #[serde(rename = "dashboard.viewed")]
    DashboardViewed,
}

/// 事件结构体
///
/// GhostCode 事件溯源的基础数据单元
/// 每个事件代表一个不可变的状态变更记录
///
/// 字段说明：
/// - v: 协议版本号，固定为 1
/// - id: uuid v4 hex 格式（32 字符，无连字符）
/// - ts: ISO 8601 UTC 微秒精度时间戳
/// - kind: 事件类型
/// - group_id: 所属 Group 标识
/// - scope_key: 作用域键
/// - by: 触发者（actor_id 或 "user"）
/// - data: 事件负载，任意 JSON 值
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// 协议版本号，固定为 1
    pub v: u8,
    /// 事件唯一标识，uuid v4 hex（32 字符）
    pub id: String,
    /// ISO 8601 UTC 微秒精度时间戳
    pub ts: String,
    /// 事件类型
    pub kind: EventKind,
    /// 所属 Group 标识
    pub group_id: String,
    /// 作用域键
    pub scope_key: String,
    /// 触发者（actor_id 或 "user"）
    pub by: String,
    /// 事件负载数据
    pub data: serde_json::Value,
}

impl Event {
    /// 创建新事件
    ///
    /// 自动生成 uuid v4 hex ID 和 ISO 8601 UTC 微秒精度时间戳
    ///
    /// @param kind - 事件类型
    /// @param group_id - 所属 Group 标识
    /// @param scope_key - 作用域键
    /// @param by - 触发者标识
    /// @param data - 事件负载数据
    pub fn new(
        kind: EventKind,
        group_id: impl Into<String>,
        scope_key: impl Into<String>,
        by: impl Into<String>,
        data: serde_json::Value,
    ) -> Self {
        Self {
            v: 1,
            id: uuid::Uuid::new_v4().simple().to_string(),
            ts: chrono::Utc::now()
                .to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
            kind,
            group_id: group_id.into(),
            scope_key: scope_key.into(),
            by: by.into(),
            data,
        }
    }
}

impl EventKind {
    /// 返回所有 EventKind 变体（用于测试遍历）
    pub fn all_variants() -> &'static [EventKind] {
        &[
            EventKind::GroupCreate,
            EventKind::GroupUpdate,
            EventKind::GroupStart,
            EventKind::GroupStop,
            EventKind::GroupSetState,
            EventKind::ActorAdd,
            EventKind::ActorUpdate,
            EventKind::ActorStart,
            EventKind::ActorStop,
            EventKind::ActorRemove,
            EventKind::ChatMessage,
            EventKind::ChatRead,
            EventKind::ChatAck,
            EventKind::SystemNotify,
            EventKind::SkillLearned,
            EventKind::SkillPromoted,
            EventKind::SkillRejected,
            EventKind::RouteStart,
            EventKind::RouteComplete,
            EventKind::RouteError,
            EventKind::DashboardViewed,
        ]
    }
}
