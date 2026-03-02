//! Group 相关类型定义
//!
//! GroupState、GroupInfo
//! 描述 Agent 工作组的状态和成员信息
//!
//! @author Atlas.oi
//! @date 2026-02-28

use serde::{Deserialize, Serialize};

use crate::actor::ActorInfo;

/// Group 状态
///
/// - Active: 正在工作
/// - Idle: 空闲等待
/// - Paused: 暂停中
/// - Stopped: 已停止
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupState {
    Active,
    Idle,
    Paused,
    Stopped,
}

/// Group 信息
///
/// 描述一个 Agent 工作组的完整状态
///
/// 字段说明：
/// - group_id: Group 唯一标识
/// - title: 标题/名称
/// - state: 当前状态
/// - actors: 成员 Actor 列表
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupInfo {
    /// Group 唯一标识
    pub group_id: String,
    /// 标题/名称
    pub title: String,
    /// 当前状态
    pub state: GroupState,
    /// 成员 Actor 列表
    pub actors: Vec<ActorInfo>,
}
