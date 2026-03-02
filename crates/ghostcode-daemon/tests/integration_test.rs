//! T19 三 Agent 端到端集成测试
//!
//! 覆盖 claude / codex / gemini 三个 Agent 在同一 Group 内的完整协作场景：
//! - 场景 1: 基本消息流（发送、回复、账本持久化、mark_all_read）
//! - 场景 2: 广播消息（空收件人广播、不含发送者、data.to 内容验证）
//! - 场景 3: 持久化恢复（AppState 重建后 inbox 数量、游标持久化）
//! - 场景 4: Agent 异常退出通知（SystemNotify 事件、ActorRemove 账本记录）
//! - 场景 5: Group 状态影响投递（Paused 不广播、Active 广播、状态转换）
//!
//! @author Atlas.oi
//! @date 2026-03-02

use std::sync::Arc;

use ghostcode_daemon::actor_mgmt::{add_actor, remove_actor};
use ghostcode_daemon::group::{create_group, set_group_state};
use ghostcode_daemon::messaging::inbox::{mark_all_read, unread_messages};
use ghostcode_daemon::messaging::send::{reply_message, send_message};
use ghostcode_daemon::server::AppState;
use ghostcode_ledger::{append_event, iter_events};
use ghostcode_types::actor::{ActorInfo, ActorRole, RuntimeKind};
use ghostcode_types::event::{Event, EventKind};
use ghostcode_types::group::{GroupInfo, GroupState};
use tempfile::TempDir;

// ============================================
// 辅助函数
// ============================================

/// 构造测试用 ActorInfo
///
/// @param actor_id - Actor ID（同时作为 display_name）
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

/// 创建标准三 Agent 测试环境
///
/// 业务逻辑：
/// 1. 创建临时目录和 groups 子目录
/// 2. 创建 AppState
/// 3. 创建 Group "Integration Test Group"
/// 4. 添加三个 Actor：claude(Foreman/Claude)、codex(Peer/Codex)、gemini(Peer/Gemini)
///
/// @return (TempDir 持有生命周期, Arc<AppState>, GroupInfo)
async fn setup() -> (TempDir, Arc<AppState>, GroupInfo) {
    // 第一步：创建临时目录结构
    let dir = TempDir::new().expect("创建临时目录失败");
    let groups_dir = dir.path().join("groups");
    std::fs::create_dir_all(&groups_dir).expect("创建 groups 目录失败");

    // 第二步：创建 AppState
    let state = Arc::new(AppState::new(groups_dir.clone()));

    // 第三步：创建 Group
    let mut group =
        create_group(&groups_dir, "Integration Test Group").expect("创建 Group 失败");

    // 第四步：添加三个 Agent
    let claude = make_actor("claude", ActorRole::Foreman, RuntimeKind::Claude);
    let codex = make_actor("codex", ActorRole::Peer, RuntimeKind::Codex);
    let gemini = make_actor("gemini", ActorRole::Peer, RuntimeKind::Gemini);

    add_actor(&groups_dir, &mut group, claude).expect("添加 claude 失败");
    add_actor(&groups_dir, &mut group, codex).expect("添加 codex 失败");
    add_actor(&groups_dir, &mut group, gemini).expect("添加 gemini 失败");

    (dir, state, group)
}

// ============================================
// 场景 1 — 基本消息流
// ============================================

/// 场景1-测试1：claude 发送给 codex，codex inbox 有 1 条未读，gemini 无未读
#[tokio::test]
async fn s1_claude_sends_to_codex_codex_has_unread() {
    let (_dir, state, group) = setup().await;

    send_message(
        &state,
        &group.group_id,
        "claude",
        vec!["codex".to_string()],
        "hello codex".to_string(),
        None,
    )
    .await
    .expect("发送消息失败");

    // codex 应有 1 条未读
    let codex_unread = unread_messages(&state, &group.group_id, "codex", 50).unwrap();
    assert_eq!(codex_unread.len(), 1, "codex 应有 1 条未读消息");

    // gemini 不应有未读（不是收件人）
    let gemini_unread = unread_messages(&state, &group.group_id, "gemini", 50).unwrap();
    assert_eq!(gemini_unread.len(), 0, "gemini 不应有未读消息");
}

/// 场景1-测试2：codex 回复 claude，reply_to 指向原始消息，claude inbox 有回复
#[tokio::test]
async fn s1_codex_replies_claude_has_reply_in_inbox() {
    let (_dir, state, group) = setup().await;

    // claude 发送原始消息
    let original = send_message(
        &state,
        &group.group_id,
        "claude",
        vec!["codex".to_string()],
        "original message".to_string(),
        None,
    )
    .await
    .expect("发送原始消息失败");

    // codex 回复 claude
    let reply = reply_message(
        &state,
        &group.group_id,
        "codex",
        &original.id,
        "got it".to_string(),
    )
    .await
    .expect("回复消息失败");

    // 验证 reply_to 链接到原始消息 ID
    assert_eq!(
        reply.data["reply_to"].as_str(),
        Some(original.id.as_str()),
        "reply_to 应链接到原始消息 ID"
    );

    // claude 的 inbox 应有 1 条未读（codex 的回复）
    let claude_unread = unread_messages(&state, &group.group_id, "claude", 50).unwrap();
    assert_eq!(claude_unread.len(), 1, "claude 应有 1 条未读回复");
}

/// 场景1-测试3：所有消息（原始+回复）在账本中均有 ChatMessage 记录
#[tokio::test]
async fn s1_all_messages_persisted_in_ledger() {
    let (_dir, state, group) = setup().await;

    let original = send_message(
        &state,
        &group.group_id,
        "claude",
        vec!["codex".to_string()],
        "original".to_string(),
        None,
    )
    .await
    .expect("发送原始消息失败");

    reply_message(
        &state,
        &group.group_id,
        "codex",
        &original.id,
        "reply".to_string(),
    )
    .await
    .expect("回复消息失败");

    // 账本中应有 2 条 ChatMessage 事件
    let ledger_path = state
        .groups_dir
        .join(&group.group_id)
        .join("state/ledger/ledger.jsonl");

    let chat_events: Vec<_> = iter_events(&ledger_path)
        .unwrap()
        .filter_map(|r| r.ok())
        .filter(|e| e.kind == EventKind::ChatMessage)
        .collect();

    assert_eq!(chat_events.len(), 2, "账本中应有 2 条 ChatMessage 事件");
}

/// 场景1-测试4：mark_all_read 后 inbox 清空
#[tokio::test]
async fn s1_mark_all_read_clears_inbox() {
    let (_dir, state, group) = setup().await;

    // claude 发送 3 条消息给 codex
    for i in 0..3 {
        send_message(
            &state,
            &group.group_id,
            "claude",
            vec!["codex".to_string()],
            format!("message {}", i + 1),
            None,
        )
        .await
        .expect("发送消息失败");
    }

    // 验证有 3 条未读
    let before = unread_messages(&state, &group.group_id, "codex", 50).unwrap();
    assert_eq!(before.len(), 3, "mark_all_read 前应有 3 条未读");

    // 全部标记已读
    mark_all_read(&state, &group.group_id, "codex").unwrap();

    // inbox 应为空
    let after = unread_messages(&state, &group.group_id, "codex", 50).unwrap();
    assert_eq!(after.len(), 0, "mark_all_read 后 inbox 应为空");
}

// ============================================
// 场景 2 — 广播
// ============================================

/// 场景2-测试1：空 recipients 广播后 codex 和 gemini 都收到消息
#[tokio::test]
async fn s2_broadcast_reaches_all_non_senders() {
    let (_dir, state, group) = setup().await;

    // claude 广播（空 recipients）
    send_message(
        &state,
        &group.group_id,
        "claude",
        vec![], // 空 = 广播
        "broadcast message".to_string(),
        None,
    )
    .await
    .expect("广播消息失败");

    // codex 和 gemini 都应收到
    let codex_unread = unread_messages(&state, &group.group_id, "codex", 50).unwrap();
    let gemini_unread = unread_messages(&state, &group.group_id, "gemini", 50).unwrap();

    assert_eq!(codex_unread.len(), 1, "codex 应收到广播消息");
    assert_eq!(gemini_unread.len(), 1, "gemini 应收到广播消息");
}

/// 场景2-测试2：广播不投递给发送者自身
#[tokio::test]
async fn s2_broadcast_not_delivered_to_sender() {
    let (_dir, state, group) = setup().await;

    // claude 广播
    send_message(
        &state,
        &group.group_id,
        "claude",
        vec![],
        "broadcast".to_string(),
        None,
    )
    .await
    .expect("广播消息失败");

    // claude 自己的 inbox 应为空
    let claude_unread = unread_messages(&state, &group.group_id, "claude", 50).unwrap();
    assert_eq!(claude_unread.len(), 0, "广播不应投递给发送者自身");
}

/// 场景2-测试3：广播事件 data.to 包含 codex 和 gemini，不含 claude
#[tokio::test]
async fn s2_broadcast_to_field_contains_all_except_sender() {
    let (_dir, state, group) = setup().await;

    let event = send_message(
        &state,
        &group.group_id,
        "claude",
        vec![],
        "broadcast".to_string(),
        None,
    )
    .await
    .expect("广播消息失败");

    let to = event.data["to"].as_array().unwrap();

    assert!(
        to.iter().any(|v| v.as_str() == Some("codex")),
        "to 应包含 codex"
    );
    assert!(
        to.iter().any(|v| v.as_str() == Some("gemini")),
        "to 应包含 gemini"
    );
    assert!(
        !to.iter().any(|v| v.as_str() == Some("claude")),
        "to 不应包含发送者 claude"
    );
}

// ============================================
// 场景 3 — 持久化恢复
// ============================================

/// 场景3-测试1：发送 10 条消息 → drop state → 重建 AppState → inbox 数量完整
#[tokio::test]
async fn s3_inbox_survives_appstate_rebuild() {
    let (dir, state, group) = setup().await;
    let groups_dir = dir.path().join("groups");
    let group_id = group.group_id.clone();

    // 发送 10 条消息给 codex
    for i in 0..10 {
        send_message(
            &state,
            &group_id,
            "claude",
            vec!["codex".to_string()],
            format!("message {}", i + 1),
            None,
        )
        .await
        .expect("发送消息失败");
    }

    // 丢弃旧的 state，模拟 Daemon 重启
    drop(state);

    // 重建 AppState
    let new_state = Arc::new(AppState::new(groups_dir.clone()));

    // 重建后 codex 的 inbox 应仍有 10 条未读
    let unread = unread_messages(&new_state, &group_id, "codex", 50).unwrap();
    assert_eq!(unread.len(), 10, "重建后 inbox 数量应完整（10 条）");
}

/// 场景3-测试2：重建后账本中 ChatMessage 数量完整
#[tokio::test]
async fn s3_ledger_chat_message_count_survives_rebuild() {
    let (dir, state, group) = setup().await;
    let groups_dir = dir.path().join("groups");
    let group_id = group.group_id.clone();

    // 发送 5 条消息
    for i in 0..5 {
        send_message(
            &state,
            &group_id,
            "claude",
            vec!["codex".to_string()],
            format!("msg {}", i),
            None,
        )
        .await
        .expect("发送消息失败");
    }

    drop(state);

    // 直接读取账本验证事件数量（不依赖 AppState）
    let ledger_path = groups_dir
        .join(&group_id)
        .join("state/ledger/ledger.jsonl");

    let chat_count = iter_events(&ledger_path)
        .unwrap()
        .filter_map(|r| r.ok())
        .filter(|e| e.kind == EventKind::ChatMessage)
        .count();

    assert_eq!(chat_count, 5, "账本中应有 5 条 ChatMessage 事件");
}

/// 场景3-测试3：mark_all_read → drop → 重建 → inbox 为空（游标持久化）
#[tokio::test]
async fn s3_cursor_persists_across_rebuild() {
    let (dir, state, group) = setup().await;
    let groups_dir = dir.path().join("groups");
    let group_id = group.group_id.clone();

    // 发送 5 条消息给 codex
    for i in 0..5 {
        send_message(
            &state,
            &group_id,
            "claude",
            vec!["codex".to_string()],
            format!("message {}", i + 1),
            None,
        )
        .await
        .expect("发送消息失败");
    }

    // codex 全部标记已读（游标写入磁盘）
    mark_all_read(&state, &group_id, "codex").unwrap();

    // 丢弃 state，模拟重启
    drop(state);

    // 重建 AppState
    let new_state = Arc::new(AppState::new(groups_dir.clone()));

    // 重建后 codex 的 inbox 应为空（游标已持久化）
    let unread = unread_messages(&new_state, &group_id, "codex", 50).unwrap();
    assert_eq!(unread.len(), 0, "重建后游标持久化，inbox 应为空");
}

// ============================================
// 场景 4 — Agent 异常退出通知
// ============================================

/// 场景4-测试1：手动写入 SystemNotify 事件，验证可被账本读取
#[tokio::test]
async fn s4_system_notify_event_readable_from_ledger() {
    let (_dir, state, group) = setup().await;

    let group_dir = state.groups_dir.join(&group.group_id);
    let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
    let lock_path = group_dir.join("state/ledger/ledger.lock");

    // 手动写入 SystemNotify 事件（模拟 Agent 异常退出通知）
    let notify_event = Event::new(
        EventKind::SystemNotify,
        &group.group_id,
        "",
        "system",
        serde_json::json!({
            "message": "codex 进程异常退出",
            "actor_id": "codex",
            "exit_code": 1,
        }),
    );
    append_event(&ledger_path, &lock_path, &notify_event).expect("写入 SystemNotify 事件失败");

    // 从账本中读取，验证存在
    let notify_events: Vec<_> = iter_events(&ledger_path)
        .unwrap()
        .filter_map(|r| r.ok())
        .filter(|e| e.kind == EventKind::SystemNotify)
        .collect();

    assert_eq!(notify_events.len(), 1, "账本中应有 1 条 SystemNotify 事件");
    assert_eq!(
        notify_events[0].data["actor_id"].as_str(),
        Some("codex"),
        "SystemNotify actor_id 应为 codex"
    );
}

/// 场景4-测试2：SystemNotify 的 by 字段为 "system"
#[tokio::test]
async fn s4_system_notify_by_field_is_system() {
    let (_dir, state, group) = setup().await;

    let group_dir = state.groups_dir.join(&group.group_id);
    let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
    let lock_path = group_dir.join("state/ledger/ledger.lock");

    // 写入 SystemNotify 事件，by = "system"
    let notify_event = Event::new(
        EventKind::SystemNotify,
        &group.group_id,
        "",
        "system",
        serde_json::json!({ "message": "gemini 心跳超时" }),
    );
    append_event(&ledger_path, &lock_path, &notify_event).expect("写入 SystemNotify 失败");

    // 验证 by 字段为 "system"
    let events: Vec<_> = iter_events(&ledger_path)
        .unwrap()
        .filter_map(|r| r.ok())
        .filter(|e| e.kind == EventKind::SystemNotify)
        .collect();

    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].by,
        "system",
        "SystemNotify 的 by 字段应为 system"
    );
}

/// 场景4-测试3：remove_actor 后账本有 ActorRemove 事件
#[tokio::test]
async fn s4_remove_actor_creates_actor_remove_event() {
    let (_dir, state, mut group) = setup().await;

    // 移除 gemini
    remove_actor(&state.groups_dir, &mut group, "gemini").expect("移除 gemini 失败");

    // 验证账本中有 ActorRemove 事件
    let ledger_path = state
        .groups_dir
        .join(&group.group_id)
        .join("state/ledger/ledger.jsonl");

    let remove_events: Vec<_> = iter_events(&ledger_path)
        .unwrap()
        .filter_map(|r| r.ok())
        .filter(|e| e.kind == EventKind::ActorRemove)
        .collect();

    assert_eq!(remove_events.len(), 1, "账本中应有 1 条 ActorRemove 事件");
    assert_eq!(
        remove_events[0].data["actor_id"].as_str(),
        Some("gemini"),
        "ActorRemove actor_id 应为 gemini"
    );
}

// ============================================
// 场景 5 — Group 状态影响投递
// ============================================

/// 场景5-测试1：Paused 状态发消息，写入账本但 event_tx 不广播
#[tokio::test]
async fn s5_paused_group_no_broadcast() {
    let (_dir, state, mut group) = setup().await;

    // 设置 Group 为 Paused 状态
    set_group_state(&state.groups_dir, &mut group, GroupState::Paused)
        .expect("设置 Paused 状态失败");

    // 订阅广播通道
    let mut rx = state.event_tx.subscribe();

    // Paused 状态下发送消息
    send_message(
        &state,
        &group.group_id,
        "claude",
        vec!["codex".to_string()],
        "paused msg".to_string(),
        None,
    )
    .await
    .expect("Paused 状态发送应成功");

    // 验证没有广播事件（Paused 时跳过 event_tx.send）
    assert!(
        rx.try_recv().is_err(),
        "Paused 状态不应通过 event_tx 广播"
    );
}

/// 场景5-测试2：Paused 时写入的消息仍可通过 inbox 查询
#[tokio::test]
async fn s5_paused_message_still_in_ledger_and_inbox() {
    let (_dir, state, mut group) = setup().await;

    // 设置 Group 为 Paused 状态
    set_group_state(&state.groups_dir, &mut group, GroupState::Paused)
        .expect("设置 Paused 状态失败");

    // Paused 状态下发送消息
    send_message(
        &state,
        &group.group_id,
        "claude",
        vec!["codex".to_string()],
        "paused msg".to_string(),
        None,
    )
    .await
    .expect("Paused 状态发送应成功");

    // 验证账本中有 ChatMessage 事件
    let ledger_path = state
        .groups_dir
        .join(&group.group_id)
        .join("state/ledger/ledger.jsonl");

    let chat_events: Vec<_> = iter_events(&ledger_path)
        .unwrap()
        .filter_map(|r| r.ok())
        .filter(|e| e.kind == EventKind::ChatMessage)
        .collect();

    assert!(
        !chat_events.is_empty(),
        "Paused 状态消息应写入账本"
    );

    // 验证仍可通过 inbox 查询
    let unread = unread_messages(&state, &group.group_id, "codex", 50).unwrap();
    assert_eq!(unread.len(), 1, "Paused 时写入的消息应可通过 inbox 查询");
}

/// 场景5-测试3：Active 状态发消息，event_tx 有广播
#[tokio::test]
async fn s5_active_group_broadcasts_event() {
    let (_dir, state, mut group) = setup().await;

    // 设置 Group 为 Active 状态
    set_group_state(&state.groups_dir, &mut group, GroupState::Active)
        .expect("设置 Active 状态失败");

    // 订阅广播通道
    let mut rx = state.event_tx.subscribe();

    // Active 状态下发送消息
    send_message(
        &state,
        &group.group_id,
        "claude",
        vec!["codex".to_string()],
        "active msg".to_string(),
        None,
    )
    .await
    .expect("Active 状态发送失败");

    // 验证广播通道有事件
    let received = rx.try_recv();
    assert!(
        received.is_ok(),
        "Active 状态应通过 event_tx 广播事件"
    );

    let received_event = received.unwrap();
    assert_eq!(
        received_event.kind,
        EventKind::ChatMessage,
        "广播的事件应为 ChatMessage"
    );
}

/// 场景5-测试4：Paused → Active 状态转换后广播恢复
#[tokio::test]
async fn s5_paused_to_active_resumes_broadcast() {
    let (_dir, state, mut group) = setup().await;

    // 先设置为 Paused
    set_group_state(&state.groups_dir, &mut group, GroupState::Paused)
        .expect("设置 Paused 状态失败");

    // 订阅广播通道
    let mut rx = state.event_tx.subscribe();

    // Paused 时发消息（不广播）
    send_message(
        &state,
        &group.group_id,
        "claude",
        vec!["codex".to_string()],
        "paused msg".to_string(),
        None,
    )
    .await
    .expect("Paused 时发送应成功");

    // 验证没有广播
    assert!(
        rx.try_recv().is_err(),
        "Paused 时不应有广播"
    );

    // 切换为 Active
    set_group_state(&state.groups_dir, &mut group, GroupState::Active)
        .expect("切换 Active 状态失败");

    // Active 时发消息（应广播）
    send_message(
        &state,
        &group.group_id,
        "claude",
        vec!["codex".to_string()],
        "active msg after resume".to_string(),
        None,
    )
    .await
    .expect("Active 时发送失败");

    // 验证广播通道有事件
    assert!(
        rx.try_recv().is_ok(),
        "Paused → Active 转换后广播应恢复"
    );
}
