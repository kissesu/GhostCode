//! T11 消息发送 + 事件写入测试套件
//!
//! 覆盖 send_message/reply_message 核心场景 + 边界情况
//!
//! @author Atlas.oi
//! @date 2026-03-01

use std::sync::Arc;

use ghostcode_daemon::actor_mgmt::add_actor;
use ghostcode_daemon::group::{create_group, set_group_state};
use ghostcode_daemon::messaging::send::{reply_message, send_message};
use ghostcode_daemon::server::AppState;
use ghostcode_types::actor::{ActorInfo, ActorRole, RuntimeKind};
use ghostcode_types::event::EventKind;
use ghostcode_types::group::{GroupInfo, GroupState};
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
    // ============================================
    // 第一步：创建临时目录和 groups 目录
    // ============================================
    let dir = TempDir::new().expect("创建临时目录失败");
    let groups_dir = dir.path().join("groups");
    std::fs::create_dir_all(&groups_dir).expect("创建 groups 目录失败");

    // ============================================
    // 第二步：创建 AppState
    // ============================================
    let state = Arc::new(AppState::new(groups_dir.clone()));

    // ============================================
    // 第三步：创建 Group
    // ============================================
    let mut group = create_group(&groups_dir, "Test Group").expect("创建 Group 失败");

    // ============================================
    // 第四步：添加 3 个 Actor
    // claude 为 Foreman，codex 和 gemini 为 Peer
    // ============================================
    let claude = make_actor("claude", ActorRole::Foreman, RuntimeKind::Claude);
    let codex = make_actor("codex", ActorRole::Peer, RuntimeKind::Codex);
    let gemini = make_actor("gemini", ActorRole::Peer, RuntimeKind::Gemini);

    add_actor(&groups_dir, &mut group, claude).expect("添加 claude 失败");
    add_actor(&groups_dir, &mut group, codex).expect("添加 codex 失败");
    add_actor(&groups_dir, &mut group, gemini).expect("添加 gemini 失败");

    (dir, state, group)
}

// ============================================
// 测试用例
// ============================================

/// 测试 1：send_message 后消息应持久化到账本
///
/// 验证：发送消息后账本中存在对应的 ChatMessage 事件
#[tokio::test]
async fn send_message_persisted() {
    let (_dir, state, group) = setup().await;

    // claude 发送消息给 codex
    let event = send_message(
        &state,
        &group.group_id,
        "claude",
        vec!["codex".to_string()],
        "hello codex".to_string(),
        None,
    )
    .await
    .expect("发送消息失败");

    // ============================================
    // 验证账本中有该 ChatMessage 事件
    // ============================================
    let ledger_path = state
        .groups_dir
        .join(&group.group_id)
        .join("state/ledger/ledger.jsonl");

    let events: Vec<_> = iter_events(&ledger_path)
        .unwrap()
        .filter_map(|r| r.ok())
        .filter(|e| e.kind == EventKind::ChatMessage)
        .collect();

    assert_eq!(events.len(), 1, "应有 1 条 ChatMessage 事件");
    assert_eq!(events[0].id, event.id);
    assert_eq!(events[0].data["text"], "hello codex");
}

/// 测试 2：指定收件人时 to 字段只包含指定 Actor
///
/// 验证：发送给指定收件人时，事件 data 中 to 字段只包含该收件人
#[tokio::test]
async fn send_to_specific_recipient() {
    let (_dir, state, group) = setup().await;

    let event = send_message(
        &state,
        &group.group_id,
        "claude",
        vec!["codex".to_string()],
        "for codex only".to_string(),
        None,
    )
    .await
    .expect("发送消息失败");

    // 验证 to 只包含 codex，不包含 gemini
    let to = event.data["to"].as_array().unwrap();
    assert!(
        to.iter().any(|v| v.as_str() == Some("codex")),
        "to 应包含 codex"
    );
    assert!(
        !to.iter().any(|v| v.as_str() == Some("gemini")),
        "to 不应包含 gemini"
    );
}

/// 测试 3：空收件人列表时广播给除发送者外所有 Actor
///
/// 验证：recipients 为空时，to 包含除 sender 之外的所有 Actor
#[tokio::test]
async fn broadcast_to_all() {
    let (_dir, state, group) = setup().await;

    // 空 recipients = 广播
    let event = send_message(
        &state,
        &group.group_id,
        "claude",
        vec![],
        "broadcast msg".to_string(),
        None,
    )
    .await
    .expect("广播消息失败");

    let to = event.data["to"].as_array().unwrap();
    assert!(
        to.iter().any(|v| v.as_str() == Some("codex")),
        "广播应包含 codex"
    );
    assert!(
        to.iter().any(|v| v.as_str() == Some("gemini")),
        "广播应包含 gemini"
    );
    assert!(
        !to.iter().any(|v| v.as_str() == Some("claude")),
        "广播不应包含发送者 claude"
    );
}

/// 测试 4：reply_message 应通过 reply_to 链接到原始消息
///
/// 验证：回复事件的 reply_to 字段指向原始事件 ID，且包含 quote_text
#[tokio::test]
async fn reply_links_to_original() {
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

    // codex 回复
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
        "reply_to 应链接到原始消息"
    );

    // 验证 quote_text 存在且包含原始文本内容
    assert!(
        reply.data["quote_text"].as_str().is_some(),
        "应包含 quote_text"
    );
    assert!(
        reply.data["quote_text"]
            .as_str()
            .unwrap()
            .contains("original message"),
        "quote_text 应包含原始文本"
    );
}

/// 测试 5：超过 32KB 的大消息应触发 blob 溢出
///
/// 验证：50KB 消息发送后
/// - blob 文件存在于 blobs/ 目录
/// - Event.data 包含 _blob_ref 字段
/// - blob 文件内容等于原始文本
#[tokio::test]
async fn large_message_blob_spill() {
    let (_dir, state, group) = setup().await;

    // 构造 50KB 文本（远超 32KB 阈值）
    let large_body = "x".repeat(50 * 1024);

    let event = send_message(
        &state,
        &group.group_id,
        "claude",
        vec!["codex".to_string()],
        large_body.clone(),
        None,
    )
    .await
    .expect("发送大消息失败");

    // ============================================
    // 验证 blob 文件存在
    // ============================================
    let blobs_dir = state
        .groups_dir
        .join(&group.group_id)
        .join("state/ledger/blobs");
    let blob_filename = format!("chat.{}.txt", event.id);
    let blob_path = blobs_dir.join(&blob_filename);
    assert!(blob_path.exists(), "blob 文件应存在: {:?}", blob_path);

    // ============================================
    // 验证 Event.data 中包含 _blob_ref
    // ============================================
    assert!(
        event.data.get("_blob_ref").is_some(),
        "event.data 应包含 _blob_ref"
    );
    assert_eq!(
        event.data["_blob_ref"].as_str().unwrap(),
        blob_filename,
        "_blob_ref 应为正确的 blob 文件名"
    );

    // ============================================
    // 验证原始 text 字段已被移除（溢出后替换为 _blob_ref）
    // ============================================
    assert!(
        event.data.get("text").is_none(),
        "溢出后原始 text 字段应被移除"
    );
    assert!(
        event.data.get("body_preview").is_some(),
        "溢出后应包含 body_preview"
    );

    // ============================================
    // 验证 blob 文件内容等于原始文本
    // ============================================
    let blob_content = std::fs::read_to_string(&blob_path).expect("读取 blob 失败");
    assert_eq!(blob_content, large_body, "blob 内容应等于原始文本");
}

/// 测试 6：Paused 状态的 Group 发送消息时不应广播事件
///
/// 验证：Group 处于 Paused 状态时，消息写入账本但不通过 event_tx 广播
#[tokio::test]
async fn paused_group_no_delivery() {
    let (_dir, state, mut group) = setup().await;

    // 设置 Group 为 Paused 状态
    set_group_state(&state.groups_dir, &mut group, GroupState::Paused)
        .expect("设置 Paused 失败");

    // 订阅事件广播通道
    let mut rx = state.event_tx.subscribe();

    // 发送消息（Paused 状态下消息仍应写入账本）
    let _event = send_message(
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

    // 验证没有广播事件（Paused 时跳过 event_tx.send）
    assert!(
        rx.try_recv().is_err(),
        "Paused 状态不应广播事件"
    );
}

/// 测试 7：不存在的 sender 应返回错误
///
/// 验证：sender_id 不在 Group 内时 send_message 返回 SenderNotFound 错误
#[tokio::test]
async fn send_invalid_sender_rejected() {
    let (_dir, state, group) = setup().await;

    let result = send_message(
        &state,
        &group.group_id,
        "unknown_actor",
        vec![],
        "test".to_string(),
        None,
    )
    .await;

    assert!(result.is_err(), "不存在的 sender 应返回错误");

    // 验证错误消息包含关键词
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not found") || err_msg.contains("Sender") || err_msg.contains("not in group"),
        "错误应提示 sender 不存在: {}",
        err_msg
    );
}

/// 测试 8：不存在的原始事件 ID 应导致 reply_message 返回错误
///
/// 验证：reply_to_event_id 在账本中不存在时，reply_message 返回 EventNotFound 错误
#[tokio::test]
async fn reply_invalid_event_rejected() {
    let (_dir, state, group) = setup().await;

    let result = reply_message(
        &state,
        &group.group_id,
        "claude",
        "nonexistent_event_id",
        "reply".to_string(),
    )
    .await;

    assert!(result.is_err(), "不存在的 event_id 应返回错误");

    // 验证错误消息包含关键词
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not found") || err_msg.contains("Event"),
        "错误应提示 event 不存在: {}",
        err_msg
    );
}
