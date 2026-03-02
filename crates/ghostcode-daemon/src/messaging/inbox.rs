//! Inbox 读取 + 已读游标核心逻辑
//!
//! 实现未读消息查询、已读游标管理、消息 ACK
//! 游标存储在 <group_dir>/state/read_cursors.json
//! 每次 mark_read 同时写入 ChatRead 事件到账本（双写策略）
//!
//! 参考: cccc/src/cccc/kernel/inbox.py - 完整 Inbox 实现
//!
//! @author Atlas.oi
//! @date 2026-03-01

use std::collections::HashMap;
use std::path::Path;

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use ghostcode_types::event::{Event, EventKind};

use crate::server::AppState;
use super::{MessagingError, Result};

// ============================================
// 游标数据结构
// ============================================

/// 已读游标条目
/// 记录 Actor 在某个 Group 中的已读进度
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CursorEntry {
    /// 最后已读的事件 ID
    event_id: String,
    /// 最后已读事件的时间戳（ISO 8601）
    ts: String,
}

/// 已读游标集合
/// key = actor_id, value = CursorEntry
type Cursors = HashMap<String, CursorEntry>;

// ============================================
// 游标持久化工具函数
// ============================================

/// 从磁盘加载已读游标
/// 文件不存在时返回空 HashMap（首次使用场景）
/// JSON 损坏时返回错误（禁止降级回退策略，问题应暴露并修复）
///
/// 参考: cccc/src/cccc/kernel/inbox.py:38-42 load_cursors
fn load_cursors(cursors_path: &Path) -> Result<Cursors> {
    if !cursors_path.exists() {
        return Ok(HashMap::new());
    }
    let content = std::fs::read_to_string(cursors_path)
        .map_err(MessagingError::Io)?;
    serde_json::from_str(&content)
        .map_err(|e| MessagingError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData,
            format!("read_cursors.json 解析失败（文件可能损坏）: {}", e))))
}

// ============================================
// 消息过滤工具函数
// ============================================

/// 判断消息是否应该投递给指定 Actor
/// 检查 data["to"] 列表：包含 actor_id 或为空（广播模式）
///
/// 参考: cccc/src/cccc/kernel/inbox.py:395-447 is_message_for_actor
fn is_message_for_actor(event: &Event, actor_id: &str) -> bool {
    // 只处理 ChatMessage 事件
    if event.kind != EventKind::ChatMessage {
        return false;
    }
    // 排除自己发送的消息（不显示给发送者）
    if event.by == actor_id {
        return false;
    }
    // 检查收件人列表
    match event.data.get("to").and_then(|v| v.as_array()) {
        Some(recipients) => {
            // 空数组 = 广播给所有人
            if recipients.is_empty() {
                return true;
            }
            // 检查是否包含 actor_id 或 @all 特殊标记
            recipients.iter().any(|r| {
                r.as_str().map_or(false, |s| s == actor_id || s == "@all")
            })
        }
        // 没有 to 字段 = 广播模式
        None => true,
    }
}

// ============================================
// 公共 API
// ============================================

/// 查询未读消息列表
///
/// 业务逻辑：
/// 1. 加载游标获取 cursor_ts（上次已读时间戳）
/// 2. 遍历账本过滤 ChatMessage + is_message_for_actor + ts > cursor_ts
/// 3. 限制返回数量（limit = 0 表示不限制）
///
/// 参考: cccc/src/cccc/kernel/inbox.py unread_messages
///
/// @param state - 共享应用状态
/// @param group_id - Group ID
/// @param actor_id - 查询者 Actor ID
/// @param limit - 最大返回数量，0 表示不限制
/// @return 未读消息事件列表
pub fn unread_messages(
    state: &AppState,
    group_id: &str,
    actor_id: &str,
    limit: usize,
) -> Result<Vec<Event>> {
    let group_dir = state.groups_dir.join(group_id);
    let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
    let cursors_path = group_dir.join("state/read_cursors.json");

    // ============================================
    // 第一步：加载已读游标，获取上次已读时间戳
    // ============================================
    let cursors = load_cursors(&cursors_path)?;
    let cursor_ts = cursors
        .get(actor_id)
        .map(|c| c.ts.as_str())
        .unwrap_or("");

    // ============================================
    // 第二步：遍历账本，过滤出未读的目标消息
    // 条件：ChatMessage + 目标 Actor + ts > cursor_ts
    // ============================================
    let mut messages: Vec<Event> = ghostcode_ledger::iter_events(&ledger_path)?
        .filter_map(|r| r.ok())
        .filter(|e| is_message_for_actor(e, actor_id) && e.ts.as_str() > cursor_ts)
        .collect();

    // ============================================
    // 第三步：按时间戳升序排列，并应用 limit 限制
    // ============================================
    messages.sort_by(|a, b| a.ts.cmp(&b.ts));
    if limit > 0 && messages.len() > limit {
        messages.truncate(limit);
    }

    Ok(messages)
}

/// 查询未读消息数量
///
/// 同 unread_messages 逻辑但只计数，不返回消息体
/// 用于状态栏显示等轻量查询场景
///
/// @param state - 共享应用状态
/// @param group_id - Group ID
/// @param actor_id - 查询者 Actor ID
/// @return 未读消息数量
pub fn unread_count(
    state: &AppState,
    group_id: &str,
    actor_id: &str,
) -> Result<usize> {
    let group_dir = state.groups_dir.join(group_id);
    let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
    let cursors_path = group_dir.join("state/read_cursors.json");

    // 加载已读游标
    let cursors = load_cursors(&cursors_path)?;
    let cursor_ts = cursors
        .get(actor_id)
        .map(|c| c.ts.as_str())
        .unwrap_or("");

    // 遍历账本计数，不收集完整事件数据，节省内存
    let count = ghostcode_ledger::iter_events(&ledger_path)?
        .filter_map(|r| r.ok())
        .filter(|e| is_message_for_actor(e, actor_id) && e.ts.as_str() > cursor_ts)
        .count();

    Ok(count)
}

/// 标记消息已读（更新游标 + 写入 ChatRead 事件）
///
/// 业务逻辑：
/// 1. 从账本查找目标事件，获取其时间戳
/// 2. 单调性检查：新 ts <= 当前游标 ts 时静默返回（防止游标回退）
/// 3. 写入 ChatRead 事件到账本（双写策略，保证可审计性）
/// 4. 更新 read_cursors.json
///
/// 参考: cccc/src/cccc/kernel/inbox.py mark_read
///
/// @param state - 共享应用状态
/// @param group_id - Group ID
/// @param actor_id - 操作者 Actor ID
/// @param event_id - 要标记已读的事件 ID
pub fn mark_read(
    state: &AppState,
    group_id: &str,
    actor_id: &str,
    event_id: &str,
) -> Result<()> {
    let group_dir = state.groups_dir.join(group_id);
    let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
    let lock_path = group_dir.join("state/ledger/ledger.lock");
    let cursors_path = group_dir.join("state/read_cursors.json");
    let cursors_lock_path = group_dir.join("state/read_cursors.lock");

    // ============================================
    // 第一步：从账本查找目标事件，获取时间戳
    // ============================================
    let target_event = ghostcode_ledger::iter_events(&ledger_path)?
        .filter_map(|r| r.ok())
        .find(|e| e.id == event_id)
        .ok_or_else(|| MessagingError::EventNotFound(event_id.to_string()))?;

    let new_ts = target_event.ts.clone();

    // ============================================
    // 第二步：写入 ChatRead 事件到账本（双写策略）
    // 先写账本再更新游标，保证可审计性
    // ============================================
    let read_event = Event::new(
        EventKind::ChatRead,
        group_id,
        "",
        actor_id,
        serde_json::json!({
            "event_id": event_id,
            "actor_id": actor_id,
        }),
    );
    ghostcode_ledger::append_event(&ledger_path, &lock_path, &read_event)?;

    // ============================================
    // 第三步：在同一个 flock 区间内完成 load + check + save
    // 消除 TOCTOU 竞态：并发 mark_read 不会丢失更新
    // ============================================
    {
        if let Some(parent) = cursors_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let lock_file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&cursors_lock_path)?;
        lock_file.lock_exclusive()?;

        // 在锁内加载游标
        let mut cursors = load_cursors(&cursors_path)?;

        // 单调性检查，防止游标回退
        if let Some(current) = cursors.get(actor_id) {
            if new_ts.as_str() <= current.ts.as_str() {
                lock_file.unlock()?;
                return Ok(());
            }
        }

        // 更新游标并写入
        cursors.insert(
            actor_id.to_string(),
            CursorEntry {
                event_id: event_id.to_string(),
                ts: new_ts,
            },
        );
        let json = serde_json::to_string_pretty(&cursors)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(&cursors_path, json)?;
        lock_file.unlock()?;
    }

    Ok(())
}

/// 标记所有未读消息已读
///
/// 业务逻辑：
/// 1. 查询所有未读消息
/// 2. 找到最后一条（最新时间戳）
/// 3. 调用 mark_read 标记到最后一条
/// 4. 无未读消息时静默成功
///
/// @param state - 共享应用状态
/// @param group_id - Group ID
/// @param actor_id - 操作者 Actor ID
pub fn mark_all_read(
    state: &AppState,
    group_id: &str,
    actor_id: &str,
) -> Result<()> {
    // 获取所有未读消息（不限制数量）
    let messages = unread_messages(state, group_id, actor_id, 0)?;

    // 找最后一条未读消息（已按 ts 升序排列，取最后一个）
    if let Some(last) = messages.last() {
        let last_id = last.id.clone();
        mark_read(state, group_id, actor_id, &last_id)?;
    }
    // 无未读消息时静默成功

    Ok(())
}

/// ACK 消息（确认收到，写入 ChatAck 事件）
///
/// 业务逻辑：
/// 1. 验证目标事件存在于账本中
/// 2. 幂等检查：已存在相同 event_id 的 ChatAck 则静默返回
/// 3. 写入 ChatAck 事件到账本
///
/// 参考: cccc/src/cccc/kernel/inbox.py ack_message
///
/// @param state - 共享应用状态
/// @param group_id - Group ID
/// @param actor_id - ACK 操作者 Actor ID
/// @param event_id - 要 ACK 的事件 ID
pub fn ack_message(
    state: &AppState,
    group_id: &str,
    actor_id: &str,
    event_id: &str,
) -> Result<()> {
    let group_dir = state.groups_dir.join(group_id);
    let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
    let lock_path = group_dir.join("state/ledger/ledger.lock");

    // ============================================
    // 第一步：单次扫描同时验证事件存在 + 幂等检查
    // 合并两次 O(n) 全量扫描为一次，提升大账本场景性能
    // ============================================
    let mut event_exists = false;
    let mut already_acked = false;
    for event in ghostcode_ledger::iter_events(&ledger_path)?.filter_map(|r| r.ok()) {
        if event.id == event_id {
            event_exists = true;
        }
        if event.kind == EventKind::ChatAck {
            let eid_match = event.data.get("event_id")
                .and_then(|v| v.as_str())
                .map_or(false, |s| s == event_id);
            let aid_match = event.data.get("actor_id")
                .and_then(|v| v.as_str())
                .map_or(false, |s| s == actor_id);
            if eid_match && aid_match {
                already_acked = true;
            }
        }
        // 两个条件都已满足时提前退出
        if event_exists && already_acked {
            break;
        }
    }

    if !event_exists {
        return Err(MessagingError::EventNotFound(event_id.to_string()));
    }
    if already_acked {
        return Ok(());
    }

    // ============================================
    // 第三步：写入 ChatAck 事件到账本
    // ============================================
    let ack_event = Event::new(
        EventKind::ChatAck,
        group_id,
        "",
        actor_id,
        serde_json::json!({
            "event_id": event_id,
            "actor_id": actor_id,
        }),
    );
    ghostcode_ledger::append_event(&ledger_path, &lock_path, &ack_event)?;

    Ok(())
}
