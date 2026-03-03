//! 账本只读查询层
//!
//! 提供 Dashboard 所需的三种只读聚合查询函数：
//! - timeline_page: 分页读取时间线事件
//! - aggregate_agent_status: 统计每个 Actor 的最后已知状态
//! - build_history_projection: 构建 Group 的完整 Dashboard 快照
//!
//! 这些函数是纯只读操作，不修改账本文件
//!
//! @author Atlas.oi
//! @date 2026-03-03

use std::collections::HashMap;
use std::path::Path;

use ghostcode_types::dashboard::{
    AgentStatusView, DashboardSnapshot, LedgerTimelineItem, TimelinePage,
};
use ghostcode_types::event::Event;

use crate::{iter_events, Result};

// ============================================
// 内部辅助：将 Event 转换为 LedgerTimelineItem
// ============================================

/// 将 Event 转换为 Dashboard 时间线条目
///
/// data_summary 截断到最多 200 字符，防止大 payload 影响 API 响应体积
fn event_to_timeline_item(event: &Event) -> LedgerTimelineItem {
    // 将 EventKind 序列化为字符串表示（如 "chat.message"）
    let kind_str = serde_json::to_string(&event.kind)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string();

    // 事件负载摘要，超过 200 字符截断并加省略号
    let data_str = serde_json::to_string(&event.data).unwrap_or_default();
    let data_summary = if data_str.len() > 200 {
        format!("{}...", &data_str[..200])
    } else {
        data_str
    };

    LedgerTimelineItem {
        id: event.id.clone(),
        ts: event.ts.clone(),
        kind: kind_str,
        group_id: event.group_id.clone(),
        by: event.by.clone(),
        data_summary,
    }
}

// ============================================
// 公共查询函数
// ============================================

/// 分页读取账本时间线
///
/// 业务逻辑：
/// 1. 顺序读取账本所有事件，转换为 LedgerTimelineItem
/// 2. 根据 cursor（事件 id）定位分页起点
/// 3. 返回 page_size 条记录 + 下一页游标
///
/// @param ledger_path - 账本文件路径
/// @param page_size - 每页条目数（最大 100）
/// @param cursor - 分页游标（上一页最后一条 event id，None 表示从头开始）
/// @returns TimelinePage 分页结果
pub fn timeline_page(
    ledger_path: &Path,
    page_size: usize,
    cursor: Option<String>,
) -> Result<TimelinePage> {
    // 限制每页最大 100 条，防止单次查询过大
    let page_size = page_size.min(100);

    // 全量读取账本事件到内存（对于 Dashboard 场景，账本规模可控）
    let items_all: Vec<Event> = iter_events(ledger_path)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let total = items_all.len() as u64;

    // 确定分页起始位置：如有 cursor 则从该 id 的下一条开始
    let start = if let Some(ref cursor_id) = cursor {
        items_all
            .iter()
            .position(|e| e.id == *cursor_id)
            .map(|pos| pos + 1)
            .unwrap_or(0)
    } else {
        0
    };

    // 截取本页数据
    let slice = &items_all[start..];
    let items: Vec<LedgerTimelineItem> = slice
        .iter()
        .take(page_size)
        .map(event_to_timeline_item)
        .collect();

    // 判断是否还有下一页：起始位置 + 页大小 < 总数 则有下一页
    let next_cursor = if start + page_size < items_all.len() {
        items.last().map(|i| i.id.clone())
    } else {
        None
    };

    Ok(TimelinePage {
        items,
        next_cursor,
        total,
    })
}

/// 统计每个 Actor 的最后已知状态
///
/// 业务逻辑：
/// 顺序扫描账本，对每个 by 字段（触发者）记录最后一次出现的事件时间和事件类型，
/// 基于最后一条事件的 kind 推断 Actor 状态。跳过 "user" 触发者，只统计 Actor。
///
/// W3 修复：原实现所有 actor 都硬编码为 "active"，
/// 改为基于最后一条事件类型推导：
/// - ActorStop -> "stopped"
/// - ActorRemove -> "removed"
/// - 其他有效事件 -> "active"
///
/// @param ledger_path - 账本文件路径
/// @returns 每个 Actor 的状态视图列表
pub fn aggregate_agent_status(ledger_path: &Path) -> Result<Vec<AgentStatusView>> {
    // 记录每个 actor 最后一次事件的 (时间戳, 事件类型)
    let mut last_event: HashMap<String, (String, ghostcode_types::event::EventKind)> =
        HashMap::new();

    for event in iter_events(ledger_path)? {
        let event = event?;
        // 跳过 "user" 触发者和空字符串，只统计 Actor
        if event.by != "user" && !event.by.is_empty() {
            last_event.insert(event.by.clone(), (event.ts.clone(), event.kind.clone()));
        }
    }

    let agents = last_event
        .into_iter()
        .map(|(actor_id, (last_ts, last_kind))| {
            // W3 修复：基于最后一条事件类型推导 actor 状态
            let status = match last_kind {
                ghostcode_types::event::EventKind::ActorStop => "stopped".to_string(),
                ghostcode_types::event::EventKind::ActorRemove => "removed".to_string(),
                _ => "active".to_string(),
            };
            AgentStatusView {
                runtime: infer_runtime(&actor_id),
                actor_id,
                status,
                last_seen: Some(last_ts),
            }
        })
        .collect();

    Ok(agents)
}

/// 推断 Actor 的 Runtime 类型（基于 actor_id 命名约定）
///
/// 简单前缀匹配：包含 "codex" -> codex, "gemini" -> gemini,
/// "claude" -> claude, 其余 -> custom
fn infer_runtime(actor_id: &str) -> String {
    let lower = actor_id.to_lowercase();
    if lower.contains("codex") {
        "codex".to_string()
    } else if lower.contains("gemini") {
        "gemini".to_string()
    } else if lower.contains("claude") {
        "claude".to_string()
    } else {
        "custom".to_string()
    }
}

/// 构建 Group 的完整 Dashboard 快照
///
/// 业务逻辑：
/// 1. 获取 Agent 状态列表
/// 2. 读取全部事件，取最后 20 条作为 recent_timeline（W2 修复）
/// 3. 生成快照时间戳
/// 4. 组装 DashboardSnapshot
///
/// W2 修复：原实现调用 timeline_page(20, None) 从头开始取最早 20 条，
/// 改为读取全部事件后取尾部 20 条，保证 recent_timeline 是最新数据
///
/// @param ledger_path - 账本文件路径
/// @param group_id - 所属 Group ID
/// @returns DashboardSnapshot 聚合快照
pub fn build_history_projection(ledger_path: &Path, group_id: &str) -> Result<DashboardSnapshot> {
    let agents = aggregate_agent_status(ledger_path)?;

    // W2 修复：读取全部事件，取最后 20 条（最近的），而非从头取 20 条（最早的）
    let all_events: Vec<Event> = iter_events(ledger_path)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let total_events = all_events.len() as u64;

    // 取最后 20 条（最近的事件），保持时间升序输出
    let recent_timeline: Vec<LedgerTimelineItem> = all_events
        .iter()
        .rev()
        .take(20)
        .rev() // 恢复时间升序
        .map(event_to_timeline_item)
        .collect();

    Ok(DashboardSnapshot {
        group_id: group_id.to_string(),
        snapshot_ts: chrono::Utc::now()
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        total_events,
        agents,
        recent_timeline,
    })
}
