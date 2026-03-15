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
/// 将事件转换为 Timeline 条目，data_summary 返回完整的事件负载 JSON
fn event_to_timeline_item(event: &Event) -> LedgerTimelineItem {
    // 将 EventKind 序列化为字符串表示（如 "chat.message"）
    let kind_str = serde_json::to_string(&event.kind)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string();

    // 完整事件负载，不截断——截断的信息没有意义
    let data_summary = serde_json::to_string(&event.data).unwrap_or_default();

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

/// 账本事件最大加载数量
/// 防止异常大账本导致 OOM，超出时截断尾部旧事件（保留最新）
/// 10000 条事件约占 5-10 MB 内存，对开发工具场景绰绰有余
const MAX_LEDGER_EVENTS: usize = 10_000;

/// 分页读取账本时间线
///
/// 业务逻辑：
/// 1. 顺序读取账本所有事件，转换为 LedgerTimelineItem
/// 2. 根据 cursor（事件 id）定位分页起点
/// 3. 返回 page_size 条记录 + 下一页游标
///
/// W2-review 安全上限：最多加载 MAX_LEDGER_EVENTS 条事件，
/// 超出时仅保留最新的事件，total 仍反映实际总数
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

    // 全量读取账本事件到内存
    // W2-review：添加安全上限，防止异常大账本导致 OOM
    let mut items_all: Vec<Event> = iter_events(ledger_path)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let total = items_all.len() as u64;

    // 超过上限时截断：丢弃头部旧事件，保留最新的 MAX_LEDGER_EVENTS 条
    if items_all.len() > MAX_LEDGER_EVENTS {
        let skip = items_all.len() - MAX_LEDGER_EVENTS;
        items_all.drain(..skip);
    }

    // 倒序排列：最新事件在前（Dashboard 时间线 UX 要求）
    items_all.reverse();

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
    // 缓存 actor_id -> (display_name, agent_type)，从 ActorStart 事件中提取
    // 注意：键必须与 last_event 的键保持一致（都使用 event.by），
    // 否则后续查找时 actor_meta.get(&actor_id) 会因键不对齐而静默返回 None
    let mut actor_meta: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();

    for event in iter_events(ledger_path)? {
        let event = event?;
        // 跳过 "user" 触发者和空字符串，只统计 Actor
        if event.by != "user" && !event.by.is_empty() {
            last_event.insert(event.by.clone(), (event.ts.clone(), event.kind.clone()));

            // 从 ActorStart 事件的 data 字段提取 display_name 和 agent_type
            // 使用 event.by 作为键（与 last_event 对齐），而非 data["actor_id"]
            if event.kind == ghostcode_types::event::EventKind::ActorStart {
                let dn = event.data.get("display_name").and_then(|v| v.as_str()).map(|s| s.to_string());
                let at = event.data.get("agent_type").and_then(|v| v.as_str()).map(|s| s.to_string());
                actor_meta.insert(event.by.clone(), (dn, at));
            }
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
            // 从缓存中取 display_name 和 agent_type
            let (display_name, agent_type_val) = actor_meta.get(&actor_id).cloned().unwrap_or((None, None));
            AgentStatusView {
                runtime: infer_runtime(&actor_id),
                actor_id,
                status,
                last_seen: Some(last_ts),
                display_name,
                agent_type: agent_type_val,
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
    // W2-review：添加安全上限，与 timeline_page 保持一致
    let all_events: Vec<Event> = iter_events(ledger_path)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let total_events = all_events.len() as u64;

    // 取最后 20 条（最近的事件），倒序输出（最新在前，Dashboard UX 要求）
    // 注意：此处仅取 20 条，无需截断全量数据，直接 rev().take(20) 即可
    let recent_timeline: Vec<LedgerTimelineItem> = all_events
        .iter()
        .rev()
        .take(20)
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
