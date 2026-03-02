//! 消息发送核心逻辑
//!
//! 实现 ChatMessage 事件写入账本和广播通知
//! 提供 send_message（新消息）和 reply_message（回复）两个公共接口
//! 内部通过 do_send 共享写入逻辑（验证 → blob 溢出 → 账本 → 广播）
//!
//! 参考: cccc/src/cccc/daemon/messaging/chat_ops.py - handle_send / handle_reply
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_types::event::{Event, EventKind};
use ghostcode_types::group::GroupState;

use crate::server::AppState;
use super::{MessagingError, Result};

// ============================================
// 内部实现
// ============================================

/// 内部发送实现（公共逻辑）
///
/// 业务逻辑：
/// 1. 加载 Group，验证 sender 是已注册 Actor
/// 2. 构造 Event（kind = ChatMessage）
/// 3. 对 Event.data 做 blob 溢出处理（text > 32KB）
/// 4. 调用 append_event 写入账本
/// 5. 检查 Group 状态：非 Paused 时通过 event_tx 广播
/// 6. 返回写入的 Event
///
/// @param state - 共享应用状态
/// @param group_id - Group ID
/// @param sender_id - 发送者 Actor ID（必须在 Group 内已注册）
/// @param data - 消息数据负载（JSON 对象）
/// @return 写入账本的 Event
async fn do_send(
    state: &AppState,
    group_id: &str,
    sender_id: &str,
    data: serde_json::Value,
) -> Result<Event> {
    // ============================================
    // 第一步：构造路径
    // 所有账本相关路径基于 group_dir 派生
    // ============================================
    let group_dir = state.groups_dir.join(group_id);
    let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
    let lock_path = group_dir.join("state/ledger/ledger.lock");
    let blobs_dir = group_dir.join("state/ledger/blobs");

    // ============================================
    // 第二步：加载 Group，验证 sender 存在
    // sender_id 必须是 Group 内已注册的 Actor
    // ============================================
    let group = crate::group::load_group(&group_dir)
        .map_err(|_| MessagingError::GroupNotFound(group_id.to_string()))?;

    if crate::actor_mgmt::find_actor(&group, sender_id).is_none() {
        return Err(MessagingError::SenderNotFound {
            group_id: group_id.to_string(),
            actor_id: sender_id.to_string(),
        });
    }

    // ============================================
    // 第三步：构造 ChatMessage 事件
    // scope_key Phase 1 固定空串（AMB-2 决策）
    // ============================================
    let mut event = Event::new(
        EventKind::ChatMessage,
        group_id,
        "",  // scope_key Phase 1 固定空串（AMB-2 决策）
        sender_id,
        data,
    );

    // ============================================
    // 第四步：blob 溢出处理
    // text 字段超过 32KB 时写入独立 blob 文件，账本保留引用
    // ============================================
    event.data = ghostcode_ledger::blob::maybe_spill_blob(
        &blobs_dir,
        &event.id,
        &event.kind,
        &event.data,
    )?;

    // ============================================
    // 第五步：写入账本
    // 使用 flock 保证并发安全
    // ============================================
    ghostcode_ledger::append_event(&ledger_path, &lock_path, &event)?;

    // ============================================
    // 第六步：广播事件通知
    // Group 处于 Paused 状态时不广播（避免打扰暂停中的 Actor）
    // ============================================
    if group.state != GroupState::Paused {
        let _ = state.event_tx.send(event.clone());
    }

    Ok(event)
}

// ============================================
// 公共 API
// ============================================

/// 发送消息（写入 ChatMessage 事件到账本 + 广播通知）
///
/// 业务逻辑：
/// 1. 解析收件人：recipients 为空时广播给除 sender 外所有 Actor
/// 2. 验证所有收件人在 Group 内存在（不存在的跳过并记录 warn 日志）
/// 3. 构造 data JSON 并调用 do_send
///
/// 参考: cccc/src/cccc/daemon/messaging/chat_ops.py handle_send
///
/// @param state - 共享应用状态
/// @param group_id - Group ID
/// @param sender_id - 发送者 Actor ID（必须在 Group 内已注册）
/// @param recipients - 收件人列表，空=广播给除 sender 外所有 Actor
/// @param body - 消息正文
/// @param reply_to - 回复的原始 event_id（可选）
/// @return 写入的 Event
pub async fn send_message(
    state: &AppState,
    group_id: &str,
    sender_id: &str,
    recipients: Vec<String>,
    body: String,
    reply_to: Option<String>,
) -> Result<Event> {
    // ============================================
    // 第一步：加载 Group 用于收件人解析
    // ============================================
    let group_dir = state.groups_dir.join(group_id);
    let group = crate::group::load_group(&group_dir)
        .map_err(|_| MessagingError::GroupNotFound(group_id.to_string()))?;

    // ============================================
    // 第二步：解析收件人列表
    // 空列表 = 广播给除 sender 外所有 Actor
    // 非空列表 = 校验每个收件人是否在 Group 内，不存在则跳过并记录警告
    // ============================================
    let resolved_recipients: Vec<String> = if recipients.is_empty() {
        // 广播模式：排除发送者自身
        crate::actor_mgmt::list_actors(&group)
            .iter()
            .filter(|a| a.actor_id != sender_id)
            .map(|a| a.actor_id.clone())
            .collect()
    } else {
        // 指定收件人模式：过滤无效收件人
        let mut valid = Vec::new();
        for r in &recipients {
            if crate::actor_mgmt::find_actor(&group, r).is_some() {
                valid.push(r.clone());
            } else {
                tracing::warn!(
                    "recipient '{}' not found in group '{}', skipping",
                    r,
                    group_id
                );
            }
        }
        valid
    };

    // ============================================
    // 第三步：构造消息数据负载并发送
    // ============================================
    let data = serde_json::json!({
        "text": body,
        "to": resolved_recipients,
        "reply_to": reply_to,
        "sender_id": sender_id,
    });

    do_send(state, group_id, sender_id, data).await
}

/// 回复消息（查找原始事件 + 构造回复 data + 调用 do_send）
///
/// 业务逻辑：
/// 1. 从账本查找原始事件（reply_to_event_id）
/// 2. 提取原始发送者作为默认收件人
/// 3. 提取原始文本前 100 字符作为 quote_text（用 chars 安全截断，支持中文）
/// 4. 调用 do_send 完成实际发送
///
/// 参考: cccc/src/cccc/daemon/messaging/chat_ops.py handle_reply
///
/// @param state - 共享应用状态
/// @param group_id - Group ID
/// @param sender_id - 发送者 Actor ID（必须在 Group 内已注册）
/// @param reply_to_event_id - 被回复的原始事件 ID
/// @param body - 回复正文
/// @return 写入的 Event
pub async fn reply_message(
    state: &AppState,
    group_id: &str,
    sender_id: &str,
    reply_to_event_id: &str,
    body: String,
) -> Result<Event> {
    // ============================================
    // 第一步：从账本查找原始事件
    // 逐行迭代，找到 id 匹配的事件
    // ============================================
    let ledger_path = state
        .groups_dir
        .join(group_id)
        .join("state/ledger/ledger.jsonl");

    let original_event = ghostcode_ledger::iter_events(&ledger_path)?
        .filter_map(|r| r.ok())
        .find(|e| e.id == reply_to_event_id)
        .ok_or_else(|| MessagingError::EventNotFound(reply_to_event_id.to_string()))?;

    // ============================================
    // 第二步：提取 quote_text（安全的字符截断）
    // 使用 chars() 迭代器处理 Unicode，避免字节截断导致乱码
    // 超过 100 字符时追加省略号
    // ============================================
    let quote_text = original_event
        .data
        .get("text")
        .and_then(|v| v.as_str())
        .map(|t| {
            let chars: String = t.chars().take(100).collect();
            if t.chars().count() > 100 {
                format!("{}...", chars)
            } else {
                chars
            }
        });

    // ============================================
    // 第三步：推断收件人（原始消息发送者）
    // ============================================
    let original_sender = original_event.by.clone();
    let recipients = vec![original_sender];

    // ============================================
    // 第四步：构造回复数据负载并发送
    // ============================================
    let data = serde_json::json!({
        "text": body,
        "to": recipients,
        "reply_to": reply_to_event_id,
        "quote_text": quote_text,
        "sender_id": sender_id,
    });

    do_send(state, group_id, sender_id, data).await
}
