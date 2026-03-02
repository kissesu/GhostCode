//! T12 Inbox 读取 + 已读游标测试套件
//!
//! 覆盖 unread_messages/mark_read/mark_all_read/unread_count/ack_message 核心场景 + 边界情况
//!
//! @author Atlas.oi
//! @date 2026-03-01

use std::sync::Arc;

use ghostcode_daemon::actor_mgmt::add_actor;
use ghostcode_daemon::group::create_group;
use ghostcode_daemon::messaging::send::send_message;
use ghostcode_daemon::messaging::inbox::{
    unread_messages, mark_read, mark_all_read, unread_count, ack_message,
};
use ghostcode_daemon::server::AppState;
use ghostcode_types::actor::{ActorInfo, ActorRole, RuntimeKind};
use ghostcode_types::event::EventKind;
use ghostcode_types::group::GroupInfo;
use ghostcode_ledger::iter_events;
use tempfile::TempDir;

// ============================================
// 辅助函数
// ============================================

/// 构造测试用 ActorInfo
///
/// @param actor_id - Actor ID
/// @param role - 角色（Foreman 或 Peer）
/// @param runtime - 运行时类型
fn make_actor(actor_id: &str, role: ActorRole, runtime: RuntimeKind) -> ActorInfo {
    ActorInfo {
        actor_id: actor_id.to_string(),
        display_name: actor_id.to_string(),
        role,
        runtime,
        running: false,
        pid: None,
    }
}

/// 创建测试环境：TempDir + AppState + GroupInfo（含 3 个 Actor）
///
/// 业务逻辑：
/// 1. 创建 TempDir，groups 目录在其下
/// 2. 创建 AppState::new(groups_dir)
/// 3. create_group → "Test Group"
/// 4. 添加 3 个 Actor：claude(Foreman)、codex(Peer)、gemini(Peer)
///
/// @return (TempDir 所有权保持 drop 安全, Arc<AppState>, GroupInfo)
async fn setup() -> (TempDir, Arc<AppState>, GroupInfo) {
    let dir = TempDir::new().expect("创建临时目录失败");
    let groups_dir = dir.path().join("groups");
    std::fs::create_dir_all(&groups_dir).expect("创建 groups 目录失败");

    let state = Arc::new(AppState::new(groups_dir.clone()));

    let mut group = create_group(&groups_dir, "Test Group").expect("创建 Group 失败");

    let claude = make_actor("claude", ActorRole::Foreman, RuntimeKind::Claude);
    let codex = make_actor("codex", ActorRole::Peer, RuntimeKind::Codex);
    let gemini = make_actor("gemini", ActorRole::Peer, RuntimeKind::Gemini);

    add_actor(&groups_dir, &mut group, claude).expect("添加 claude 失败");
    add_actor(&groups_dir, &mut group, codex).expect("添加 codex 失败");
    add_actor(&groups_dir, &mut group, gemini).expect("添加 gemini 失败");

    (dir, state, group)
}

/// 批量发送消息（claude -> 指定收件人）
///
/// @param state - 共享应用状态
/// @param group_id - Group ID
/// @param recipients - 收件人列表
/// @param count - 发送消息条数
/// @return 发送成功的事件列表（按发送顺序）
async fn send_n_messages(
    state: &AppState,
    group_id: &str,
    recipients: Vec<String>,
    count: usize,
) -> Vec<ghostcode_types::event::Event> {
    let mut events = Vec::new();
    for i in 0..count {
        let event = send_message(
            state,
            group_id,
            "claude",
            recipients.clone(),
            format!("message {}", i + 1),
            None,
        )
        .await
        .expect("发送消息失败");
        events.push(event);
    }
    events
}

// ============================================
// 测试用例
// ============================================

/// 测试 1：新消息应该出现在未读列表中
///
/// 验证：
/// - claude 发送 3 条给 codex 后，codex 有 3 条未读
/// - 消息内容与发送一致
/// - claude 自己没有未读（发送者不算接收方）
#[tokio::test]
async fn unread_messages_returns_new() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    // claude 发送 3 条消息给 codex
    let sent = send_n_messages(&state, group_id, vec!["codex".to_string()], 3).await;

    // codex 应该有 3 条未读
    let unread = unread_messages(&state, group_id, "codex", 50).unwrap();
    assert_eq!(unread.len(), 3);

    // 验证消息内容与发送一致
    for (i, event) in unread.iter().enumerate() {
        let text = event.data["text"].as_str().unwrap();
        assert_eq!(text, format!("message {}", i + 1));
    }

    // claude 自己不应该有未读（自己发的不算）
    let claude_unread = unread_messages(&state, group_id, "claude", 50).unwrap();
    assert_eq!(claude_unread.len(), 0);

    // 消除未使用变量警告
    let _ = sent;
}

/// 测试 2：mark_read 后游标应推进，之前的消息不再显示为未读
///
/// 验证：
/// - mark_read 到第 2 条后，只有第 3 条保持未读
/// - 游标单调推进，不会影响更新的消息
#[tokio::test]
async fn mark_read_advances_cursor() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    // claude 发送 3 条消息给 codex
    let sent = send_n_messages(&state, group_id, vec!["codex".to_string()], 3).await;

    // mark_read 到第 2 条
    let second_id = &sent[1].id;
    mark_read(&state, group_id, "codex", second_id).unwrap();

    // codex 应该只有 1 条未读（第 3 条）
    let unread = unread_messages(&state, group_id, "codex", 50).unwrap();
    assert_eq!(unread.len(), 1);
    assert_eq!(unread[0].id, sent[2].id);
}

/// 测试 3：mark_all_read 后未读列表应为空
///
/// 验证：
/// - 5 条消息全部标记已读后，unread_messages 和 unread_count 均为 0
#[tokio::test]
async fn mark_all_read_clears_inbox() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    // claude 发送 5 条消息给 codex
    send_n_messages(&state, group_id, vec!["codex".to_string()], 5).await;

    // 全部标记已读
    mark_all_read(&state, group_id, "codex").unwrap();

    // 未读应为 0
    let unread = unread_messages(&state, group_id, "codex", 50).unwrap();
    assert_eq!(unread.len(), 0);

    let count = unread_count(&state, group_id, "codex").unwrap();
    assert_eq!(count, 0);
}

/// 测试 4：unread_count 与 unread_messages 的数量应保持一致
///
/// 验证：
/// - 不同 Actor 的未读数量互相独立
/// - unread_count 和 unread_messages.len() 始终相等
#[tokio::test]
async fn unread_count_consistent() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    // claude 发送 3 条给 codex
    send_n_messages(&state, group_id, vec!["codex".to_string()], 3).await;
    // claude 发送 2 条给 gemini
    send_n_messages(&state, group_id, vec!["gemini".to_string()], 2).await;

    // codex: count == len(messages) == 3
    let codex_count = unread_count(&state, group_id, "codex").unwrap();
    let codex_messages = unread_messages(&state, group_id, "codex", 50).unwrap();
    assert_eq!(codex_count, 3);
    assert_eq!(codex_count, codex_messages.len());

    // gemini: count == len(messages) == 2
    let gemini_count = unread_count(&state, group_id, "gemini").unwrap();
    let gemini_messages = unread_messages(&state, group_id, "gemini", 50).unwrap();
    assert_eq!(gemini_count, 2);
    assert_eq!(gemini_count, gemini_messages.len());
}

/// 测试 5：游标应保持单调性，不能回退
///
/// 验证：
/// - 先 mark_read 最新消息，再 mark_read 旧消息时应被静默忽略
/// - 游标保持在最新位置，未读为 0
#[tokio::test]
async fn cursor_monotonic() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    // claude 发送 3 条消息给 codex
    let sent = send_n_messages(&state, group_id, vec!["codex".to_string()], 3).await;

    // 先 mark_read 第 3 条（最新的）
    mark_read(&state, group_id, "codex", &sent[2].id).unwrap();

    // 再 mark_read 第 1 条（尝试回退）— 应该静默忽略
    mark_read(&state, group_id, "codex", &sent[0].id).unwrap();

    // 游标应保持在第 3 条，未读为 0
    let unread = unread_messages(&state, group_id, "codex", 50).unwrap();
    assert_eq!(unread.len(), 0);
}

/// 测试 6：ack_message 应写入 ChatAck 事件，且重复 ACK 应幂等
///
/// 验证：
/// - ack_message 后账本中有 ChatAck 事件
/// - ChatAck 事件包含正确的 event_id 和 actor_id
/// - 重复 ACK 不创建新事件（幂等性）
#[tokio::test]
async fn ack_message_creates_event() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    // claude 发送 1 条消息给 codex
    let sent = send_n_messages(&state, group_id, vec!["codex".to_string()], 1).await;
    let event_id = &sent[0].id;

    // codex ACK 这条消息
    ack_message(&state, group_id, "codex", event_id).unwrap();

    // 验证账本中有 ChatAck 事件
    let ledger_path = state.groups_dir.join(group_id).join("state/ledger/ledger.jsonl");
    let ack_events: Vec<_> = iter_events(&ledger_path)
        .unwrap()
        .filter_map(|r| r.ok())
        .filter(|e| e.kind == EventKind::ChatAck)
        .collect();

    assert_eq!(ack_events.len(), 1);
    assert_eq!(ack_events[0].data["event_id"].as_str().unwrap(), event_id.as_str());
    assert_eq!(ack_events[0].data["actor_id"].as_str().unwrap(), "codex");

    // 重复 ACK 应该幂等（不创建新事件）
    ack_message(&state, group_id, "codex", event_id).unwrap();
    let ack_events2: Vec<_> = iter_events(&ledger_path)
        .unwrap()
        .filter_map(|r| r.ok())
        .filter(|e| e.kind == EventKind::ChatAck)
        .collect();
    assert_eq!(ack_events2.len(), 1); // 仍然只有 1 条
}

/// 测试 7：mark_read 应向账本写入 ChatRead 事件
///
/// 验证：
/// - mark_read 后账本中有 ChatRead 事件
/// - ChatRead 事件包含正确的 event_id 和 actor_id
#[tokio::test]
async fn mark_read_creates_chat_read_event() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    // claude 发送 1 条消息给 codex
    let sent = send_n_messages(&state, group_id, vec!["codex".to_string()], 1).await;
    let event_id = &sent[0].id;

    // codex mark_read
    mark_read(&state, group_id, "codex", event_id).unwrap();

    // 验证账本中有 ChatRead 事件
    let ledger_path = state.groups_dir.join(group_id).join("state/ledger/ledger.jsonl");
    let read_events: Vec<_> = iter_events(&ledger_path)
        .unwrap()
        .filter_map(|r| r.ok())
        .filter(|e| e.kind == EventKind::ChatRead)
        .collect();

    assert_eq!(read_events.len(), 1);
    assert_eq!(read_events[0].data["event_id"].as_str().unwrap(), event_id.as_str());
    assert_eq!(read_events[0].data["actor_id"].as_str().unwrap(), "codex");
}
