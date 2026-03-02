# Team Plan: T19 三 Agent 端到端集成测试

## 概述

为 GhostCode 核心消息系统编写端到端集成测试套件，覆盖三 Agent 协作场景。
测试文件放置在 `crates/ghostcode-daemon/tests/integration_test.rs`，
与现有单测（send_test.rs / inbox_test.rs / lifecycle_test.rs）保持相同的直接 API 调用风格——
**不启动真实 Daemon 进程，直接调用模块公共 API**，通过临时目录隔离数据。

**前置依赖**: T01-T18 全部完成（所有公共 API 已就绪）

**产出**:
- `crates/ghostcode-daemon/tests/integration_test.rs`（主测试文件，5 个场景，20 个 `#[tokio::test]`）

---

## Codex 分析摘要

Codex CLI 不可用，由 Claude 自行分析。

---

## Gemini 分析摘要

批量计划生成模式，跳过多模型分析。

---

## 技术方案

### 使用的公共 API（已验证源码）

| 模块 | 函数/类型 | 来源文件 |
|------|---------|---------|
| `ghostcode_daemon::group` | `create_group`, `set_group_state`, `load_group` | `src/group.rs` |
| `ghostcode_daemon::actor_mgmt` | `add_actor` | `src/actor_mgmt.rs` |
| `ghostcode_daemon::messaging::send` | `send_message`, `reply_message` | `src/messaging/send.rs` |
| `ghostcode_daemon::messaging::inbox` | `unread_messages`, `mark_read`, `mark_all_read` | `src/messaging/inbox.rs` |
| `ghostcode_daemon::server` | `AppState` | `src/server.rs` |
| `ghostcode_types::actor` | `ActorInfo`, `ActorRole`, `RuntimeKind` | `ghostcode-types/src/actor.rs` |
| `ghostcode_types::event` | `EventKind` | `ghostcode-types/src/event.rs` |
| `ghostcode_types::group` | `GroupInfo`, `GroupState` | `ghostcode-types/src/group.rs` |
| `ghostcode_ledger` | `iter_events` | `ghostcode-ledger/src/lib.rs` |

### setup() 辅助函数（复用现有测试模式）

所有测试场景共用同一个 `setup()` 函数，与 `send_test.rs`/`inbox_test.rs` 保持完全一致：
- 创建 `TempDir`
- 在其下创建 `groups/` 目录
- 创建 `AppState::new(groups_dir)`
- `create_group` 创建 "Integration Test Group"
- 添加 3 个 Actor：`claude`（Foreman）、`codex`（Peer）、`gemini`（Peer）
- 返回 `(TempDir, Arc<AppState>, GroupInfo)`

### 测试文件位置

```
crates/ghostcode-daemon/tests/integration_test.rs
```

与其他测试文件同级，Cargo 自动识别为集成测试（无需修改 Cargo.toml）。

### 关键实现注意点

1. **场景 4（Actor 异常退出通知）**: `SystemNotify` 事件目前通过直接写入账本验证，
   不依赖尚未实现的 Actor 进程监控逻辑。通过手动写入 `SystemNotify` 事件模拟系统通知，
   验证 `EventKind::SystemNotify` 可被 `iter_events` 正确读取。
   （Phase 1 内 Actor 异常退出的真实通知机制属于 T20 范围）

2. **场景 5（Group 状态影响投递）**: 直接复用 `send_test.rs::paused_group_no_delivery`
   已验证的模式，扩展为完整的 Paused → Active 状态转换流程，
   并通过 `event_tx.subscribe()` 验证 Active 状态下广播恢复。

3. **场景 3（持久化恢复）**: 通过"新建 AppState"模拟 Daemon 重启，
   复用 `lifecycle_test.rs::restore_running_actors_test` 的 Drop/重建模式。

4. **`_dir` 保持所有权**: 所有测试中 `TempDir` 必须绑定到有名变量（`_dir`），
   防止提前 drop 导致临时目录在测试结束前被清理。

---

## 子任务列表

### Task 1: 创建集成测试文件 + 公共 setup

**文件**: `crates/ghostcode-daemon/tests/integration_test.rs`

**内容**:
- 文件头注释（格式与其他测试文件一致）
- 全部 `use` 导入
- `make_actor()` 辅助函数
- `setup()` 辅助函数

```rust
//! T19 三 Agent 端到端集成测试
//!
//! 覆盖五个核心协作场景：
//! 1. 基本消息流（claude <-> codex 直接通信 + 账本验证）
//! 2. 广播（claude 广播 -> codex 和 gemini 都收到）
//! 3. 持久化恢复（重建 AppState 后 inbox 完整）
//! 4. Agent 异常退出通知（SystemNotify 事件写入账本）
//! 5. Group 状态影响投递（Paused 不广播 / Active 广播恢复）
//!
//! @author Atlas.oi
//! @date 2026-03-01

use std::sync::Arc;

use ghostcode_daemon::actor_mgmt::add_actor;
use ghostcode_daemon::group::{create_group, set_group_state};
use ghostcode_daemon::messaging::inbox::{mark_all_read, unread_messages};
use ghostcode_daemon::messaging::send::{reply_message, send_message};
use ghostcode_daemon::server::AppState;
use ghostcode_types::actor::{ActorInfo, ActorRole, RuntimeKind};
use ghostcode_types::event::{Event, EventKind};
use ghostcode_types::group::{GroupInfo, GroupState};
use ghostcode_ledger::iter_events;
use tempfile::TempDir;

// ============================================
// 辅助函数
// ============================================

/// 构造测试用 ActorInfo
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

/// 创建集成测试环境（3 个 Actor：claude/codex/gemini）
///
/// @return (TempDir 所有权保持 drop 安全, Arc<AppState>, GroupInfo)
async fn setup() -> (TempDir, Arc<AppState>, GroupInfo) {
    let dir = TempDir::new().expect("创建临时目录失败");
    let groups_dir = dir.path().join("groups");
    std::fs::create_dir_all(&groups_dir).expect("创建 groups 目录失败");

    let state = Arc::new(AppState::new(groups_dir.clone()));

    let mut group = create_group(&groups_dir, "Integration Test Group")
        .expect("创建 Group 失败");

    let claude = make_actor("claude", ActorRole::Foreman, RuntimeKind::Claude);
    let codex  = make_actor("codex",  ActorRole::Peer,    RuntimeKind::Codex);
    let gemini = make_actor("gemini", ActorRole::Peer,    RuntimeKind::Gemini);

    add_actor(&groups_dir, &mut group, claude).expect("添加 claude 失败");
    add_actor(&groups_dir, &mut group, codex).expect("添加 codex 失败");
    add_actor(&groups_dir, &mut group, gemini).expect("添加 gemini 失败");

    (dir, state, group)
}
```

---

### Task 2: 场景 1 — 基本消息流（4 个测试）

**包含测试**:

#### 测试 1-1: `scenario1_claude_sends_to_codex`

验证 claude 发送消息给 codex 后，codex 的 inbox 中出现该消息。

```rust
/// 场景 1-1：claude -> codex 消息后 codex inbox 中出现该消息
#[tokio::test]
async fn scenario1_claude_sends_to_codex() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    let event = send_message(
        &state,
        group_id,
        "claude",
        vec!["codex".to_string()],
        "分析后端模块".to_string(),
        None,
    )
    .await
    .expect("claude 发送消息应成功");

    // codex inbox 应有 1 条未读
    let inbox = unread_messages(&state, group_id, "codex", 50)
        .expect("读取 codex inbox 应成功");
    assert_eq!(inbox.len(), 1, "codex 应有 1 条未读");
    assert_eq!(inbox[0].id, event.id, "inbox 中的消息 ID 应匹配");
    assert_eq!(
        inbox[0].data["text"].as_str().unwrap(),
        "分析后端模块",
        "消息内容应正确"
    );

    // gemini 不是收件人，不应有未读
    let gemini_inbox = unread_messages(&state, group_id, "gemini", 50)
        .expect("读取 gemini inbox 应成功");
    assert_eq!(gemini_inbox.len(), 0, "gemini 不应有未读");
}
```

#### 测试 1-2: `scenario1_codex_replies_to_claude`

验证 codex 收到消息后通过 `reply_message` 回复 claude，claude 的 inbox 中出现回复。

```rust
/// 场景 1-2：codex 回复 claude，claude inbox 中出现回复
#[tokio::test]
async fn scenario1_codex_replies_to_claude() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    // claude 发送原始消息
    let original = send_message(
        &state, group_id, "claude",
        vec!["codex".to_string()],
        "分析后端模块".to_string(), None,
    )
    .await.expect("发送原始消息失败");

    // codex 回复
    let reply = reply_message(
        &state, group_id, "codex",
        &original.id,
        "分析完成，共 3 个模块".to_string(),
    )
    .await.expect("codex 回复应成功");

    // 验证回复 data 中 reply_to 指向原始消息
    assert_eq!(
        reply.data["reply_to"].as_str().unwrap(),
        original.id.as_str(),
        "reply_to 应指向原始消息 ID"
    );

    // claude inbox 应有 1 条未读（来自 codex 的回复）
    let claude_inbox = unread_messages(&state, group_id, "claude", 50)
        .expect("读取 claude inbox 应成功");
    assert_eq!(claude_inbox.len(), 1, "claude 应有 1 条未读");
    assert_eq!(
        claude_inbox[0].data["text"].as_str().unwrap(),
        "分析完成，共 3 个模块",
        "claude 收到的回复内容应正确"
    );
}
```

#### 测试 1-3: `scenario1_all_messages_in_ledger`

验证所有消息（原始 + 回复）均写入账本，可通过 `iter_events` 查询。

```rust
/// 场景 1-3：原始消息和回复都应持久化到账本
#[tokio::test]
async fn scenario1_all_messages_in_ledger() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    let original = send_message(
        &state, group_id, "claude",
        vec!["codex".to_string()],
        "分析后端模块".to_string(), None,
    )
    .await.expect("发送原始消息失败");

    let reply = reply_message(
        &state, group_id, "codex",
        &original.id,
        "分析完成".to_string(),
    )
    .await.expect("回复失败");

    // 账本中应有 2 条 ChatMessage 事件
    let ledger_path = state.groups_dir
        .join(group_id)
        .join("state/ledger/ledger.jsonl");

    let chat_events: Vec<_> = iter_events(&ledger_path)
        .expect("打开账本失败")
        .filter_map(|r| r.ok())
        .filter(|e| e.kind == EventKind::ChatMessage)
        .collect();

    assert_eq!(chat_events.len(), 2, "账本中应有 2 条 ChatMessage 事件");

    // 验证两条事件 ID 均匹配
    let ids: Vec<&str> = chat_events.iter().map(|e| e.id.as_str()).collect();
    assert!(ids.contains(&original.id.as_str()), "原始消息应在账本中");
    assert!(ids.contains(&reply.id.as_str()), "回复应在账本中");
}
```

#### 测试 1-4: `scenario1_mark_read_clears_inbox`

验证 codex 读取消息并调用 `mark_all_read` 后 inbox 清空。

```rust
/// 场景 1-4：codex mark_all_read 后 inbox 清空
#[tokio::test]
async fn scenario1_mark_read_clears_inbox() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    // claude 发送 2 条消息给 codex
    for i in 0..2 {
        send_message(
            &state, group_id, "claude",
            vec!["codex".to_string()],
            format!("消息 {}", i + 1), None,
        )
        .await.expect("发送失败");
    }

    // 确认 codex 有 2 条未读
    let before = unread_messages(&state, group_id, "codex", 50).unwrap();
    assert_eq!(before.len(), 2);

    // codex mark_all_read
    mark_all_read(&state, group_id, "codex").expect("mark_all_read 应成功");

    // 确认 inbox 清空
    let after = unread_messages(&state, group_id, "codex", 50).unwrap();
    assert_eq!(after.len(), 0, "mark_all_read 后 inbox 应为空");
}
```

---

### Task 3: 场景 2 — 广播（3 个测试）

#### 测试 2-1: `scenario2_broadcast_reaches_codex_and_gemini`

验证空 recipients 广播后 codex 和 gemini 的 inbox 中都出现该消息。

```rust
/// 场景 2-1：广播后 codex 和 gemini 都应收到消息
#[tokio::test]
async fn scenario2_broadcast_reaches_codex_and_gemini() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    let event = send_message(
        &state, group_id, "claude",
        vec![],  // 空 recipients = 广播
        "全员注意：开始集成测试".to_string(),
        None,
    )
    .await.expect("广播应成功");

    // codex inbox 应有 1 条
    let codex_inbox = unread_messages(&state, group_id, "codex", 50).unwrap();
    assert_eq!(codex_inbox.len(), 1, "codex 应收到广播");
    assert_eq!(codex_inbox[0].id, event.id);

    // gemini inbox 应有 1 条
    let gemini_inbox = unread_messages(&state, group_id, "gemini", 50).unwrap();
    assert_eq!(gemini_inbox.len(), 1, "gemini 应收到广播");
    assert_eq!(gemini_inbox[0].id, event.id);
}
```

#### 测试 2-2: `scenario2_broadcast_excludes_sender`

验证广播不投递给发送者自身。

```rust
/// 场景 2-2：广播不投递给发送者 claude 自身
#[tokio::test]
async fn scenario2_broadcast_excludes_sender() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    send_message(
        &state, group_id, "claude",
        vec![], "广播测试".to_string(), None,
    )
    .await.expect("广播应成功");

    // claude 自己不应有未读
    let claude_inbox = unread_messages(&state, group_id, "claude", 50).unwrap();
    assert_eq!(claude_inbox.len(), 0, "广播不应包含发送者 claude");
}
```

#### 测试 2-3: `scenario2_broadcast_to_field_contains_both`

验证广播事件的 `data.to` 字段包含 codex 和 gemini 但不含 claude。

```rust
/// 场景 2-3：广播事件 data.to 包含 codex 和 gemini，不含 claude
#[tokio::test]
async fn scenario2_broadcast_to_field_contains_both() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    let event = send_message(
        &state, group_id, "claude",
        vec![], "广播".to_string(), None,
    )
    .await.expect("广播应成功");

    let to = event.data["to"].as_array().expect("to 应为数组");
    let recipients: Vec<&str> = to.iter()
        .filter_map(|v| v.as_str())
        .collect();

    assert!(recipients.contains(&"codex"),  "to 应包含 codex");
    assert!(recipients.contains(&"gemini"), "to 应包含 gemini");
    assert!(!recipients.contains(&"claude"), "to 不应包含发送者 claude");
}
```

---

### Task 4: 场景 3 — 持久化恢复（3 个测试）

**核心思路**: 使用与 `lifecycle_test.rs::restore_running_actors_test` 相同的模式——
TempDir 持续持有，重建 AppState 模拟 Daemon 重启。

#### 测试 3-1: `scenario3_inbox_survives_restart`

发送 10 条消息 → 重建 AppState → inbox 数量完整。

```rust
/// 场景 3-1：发送 10 条消息后重建 AppState，inbox 应完整（10 条）
#[tokio::test]
async fn scenario3_inbox_survives_restart() {
    let (dir, state, group) = setup().await;
    let group_id = group.group_id.clone();
    let groups_dir = dir.path().join("groups");

    // 发送 10 条消息给 codex
    for i in 0..10 {
        send_message(
            &state, &group_id, "claude",
            vec!["codex".to_string()],
            format!("持久化消息 {}", i + 1), None,
        )
        .await.expect("发送失败");
    }

    // ============================================
    // 模拟 Daemon 重启：丢弃旧 state，重建新 AppState
    // dir 保持 alive，磁盘数据完整
    // ============================================
    drop(state);
    let new_state = Arc::new(AppState::new(groups_dir));

    // 新 state 读取 codex inbox，应有 10 条
    let inbox = unread_messages(&new_state, &group_id, "codex", 50)
        .expect("重启后读取 inbox 应成功");
    assert_eq!(inbox.len(), 10, "持久化恢复后 inbox 应有 10 条消息");
}
```

#### 测试 3-2: `scenario3_ledger_survives_restart`

验证重启后账本中 ChatMessage 事件数量完整。

```rust
/// 场景 3-2：重建 AppState 后账本中 ChatMessage 事件数量完整
#[tokio::test]
async fn scenario3_ledger_survives_restart() {
    let (dir, state, group) = setup().await;
    let group_id = group.group_id.clone();
    let groups_dir = dir.path().join("groups");

    // 发送 5 条消息（广播，codex 和 gemini 各收到 5 条）
    for i in 0..5 {
        send_message(
            &state, &group_id, "claude",
            vec![], format!("广播消息 {}", i + 1), None,
        )
        .await.expect("发送失败");
    }

    drop(state);
    let new_state = Arc::new(AppState::new(groups_dir));

    // 账本中应有 5 条 ChatMessage 事件
    let ledger_path = new_state.groups_dir
        .join(&group_id)
        .join("state/ledger/ledger.jsonl");

    let count = iter_events(&ledger_path)
        .expect("打开账本失败")
        .filter_map(|r| r.ok())
        .filter(|e| e.kind == EventKind::ChatMessage)
        .count();

    assert_eq!(count, 5, "重启后账本应有 5 条 ChatMessage 事件");
}
```

#### 测试 3-3: `scenario3_cursor_survives_restart`

验证重启前读取游标正常推进、重启后 inbox 反映游标位置。

```rust
/// 场景 3-3：mark_all_read 后重建 AppState，inbox 仍为空（游标持久化）
#[tokio::test]
async fn scenario3_cursor_survives_restart() {
    let (dir, state, group) = setup().await;
    let group_id = group.group_id.clone();
    let groups_dir = dir.path().join("groups");

    // 发送 3 条消息给 codex
    for i in 0..3 {
        send_message(
            &state, &group_id, "claude",
            vec!["codex".to_string()],
            format!("消息 {}", i + 1), None,
        )
        .await.expect("发送失败");
    }

    // codex mark_all_read（游标写入磁盘 read_cursors.json）
    mark_all_read(&state, &group_id, "codex").expect("mark_all_read 失败");

    drop(state);
    let new_state = Arc::new(AppState::new(groups_dir));

    // 重启后 codex inbox 应为空（游标已持久化）
    let inbox = unread_messages(&new_state, &group_id, "codex", 50)
        .expect("重启后读取 inbox 应成功");
    assert_eq!(inbox.len(), 0, "游标持久化后重启 inbox 应为空");
}
```

---

### Task 5: 场景 4 — Agent 异常退出通知（3 个测试）

**实现策略**: Phase 1 中 Actor 进程监控属于 T20 范围，无法真实模拟进程异常退出。
本测试直接向账本写入 `SystemNotify` 事件，验证：
1. `EventKind::SystemNotify` 可正确序列化/反序列化
2. 写入的 `SystemNotify` 事件可被 `iter_events` 读取
3. Foreman（claude）能通过 inbox 读取到 `SystemNotify`（当 to 字段包含 claude 时）

注：真实的进程异常检测和自动通知逻辑在 T20 中实现。

```rust
/// 场景 4-1：手动写入 SystemNotify 事件，验证可被账本正确持久化
#[tokio::test]
async fn scenario4_system_notify_persisted_in_ledger() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    // 构造 SystemNotify 事件（模拟 codex 异常退出）
    let notify_event = ghostcode_types::event::Event::new(
        EventKind::SystemNotify,
        group_id,
        "",         // scope_key Phase 1 固定空串
        "system",   // by = "system"
        serde_json::json!({
            "kind":     "actor_idle",
            "title":    "codex 异常退出",
            "message":  "actor codex 心跳超时，状态已标记为 Stopped",
            "actor_id": "codex",
            "to":       ["claude"],
        }),
    );

    // 写入账本
    let ledger_path = state.groups_dir
        .join(group_id)
        .join("state/ledger/ledger.jsonl");
    let lock_path = state.groups_dir
        .join(group_id)
        .join("state/ledger/ledger.lock");

    ghostcode_ledger::append_event(&ledger_path, &lock_path, &notify_event)
        .expect("写入 SystemNotify 事件应成功");

    // 从账本读回，验证 EventKind::SystemNotify 正确
    let sys_events: Vec<_> = iter_events(&ledger_path)
        .expect("打开账本失败")
        .filter_map(|r| r.ok())
        .filter(|e| e.kind == EventKind::SystemNotify)
        .collect();

    assert_eq!(sys_events.len(), 1, "应有 1 条 SystemNotify 事件");
    assert_eq!(
        sys_events[0].data["actor_id"].as_str().unwrap(),
        "codex",
        "SystemNotify 应包含 actor_id=codex"
    );
    assert_eq!(
        sys_events[0].data["kind"].as_str().unwrap(),
        "actor_idle",
        "通知类型应为 actor_idle"
    );
}

/// 场景 4-2：验证 SystemNotify 事件的 by 字段为 system
#[tokio::test]
async fn scenario4_system_notify_by_system() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    let notify_event = ghostcode_types::event::Event::new(
        EventKind::SystemNotify,
        group_id, "",
        "system",
        serde_json::json!({
            "kind": "actor_idle",
            "title": "gemini 退出",
            "message": "gemini 心跳超时",
            "actor_id": "gemini",
            "to": ["claude"],
        }),
    );

    let ledger_path = state.groups_dir
        .join(group_id)
        .join("state/ledger/ledger.jsonl");
    let lock_path = state.groups_dir
        .join(group_id)
        .join("state/ledger/ledger.lock");

    ghostcode_ledger::append_event(&ledger_path, &lock_path, &notify_event)
        .expect("写入失败");

    // 验证 by 字段为 "system"
    let events: Vec<_> = iter_events(&ledger_path)
        .unwrap()
        .filter_map(|r| r.ok())
        .filter(|e| e.kind == EventKind::SystemNotify)
        .collect();

    assert_eq!(events[0].by, "system", "SystemNotify by 字段应为 system");
}

/// 场景 4-3：3 个 Actor 注册后移除 codex，账本应有 ActorRemove 事件
///
/// 模拟 codex "异常退出"后被系统注销的账本记录
#[tokio::test]
async fn scenario4_actor_remove_recorded_in_ledger() {
    let (_dir, state, mut group) = setup().await;
    let group_id = &group.group_id;

    // 确认有 3 个 Actor
    assert_eq!(group.actors.len(), 3, "初始应有 3 个 Actor");

    // 移除 codex（模拟异常退出后注销）
    ghostcode_daemon::actor_mgmt::remove_actor(
        &state.groups_dir, &mut group, "codex"
    ).expect("移除 codex 应成功");

    assert_eq!(group.actors.len(), 2, "移除后应有 2 个 Actor");

    // 验证账本中有 ActorRemove 事件
    let ledger_path = state.groups_dir
        .join(group_id)
        .join("state/ledger/ledger.jsonl");

    let remove_events: Vec<_> = iter_events(&ledger_path)
        .unwrap()
        .filter_map(|r| r.ok())
        .filter(|e| e.kind == EventKind::ActorRemove)
        .collect();

    assert_eq!(remove_events.len(), 1, "应有 1 条 ActorRemove 事件");
    assert_eq!(
        remove_events[0].data["actor_id"].as_str().unwrap(),
        "codex",
        "ActorRemove 应记录 codex"
    );
}
```

---

### Task 6: 场景 5 — Group 状态影响投递（4 个测试）

#### 测试 5-1: `scenario5_paused_group_no_broadcast`

Paused 状态下发送消息，消息写入账本但不通过 event_tx 广播。

```rust
/// 场景 5-1：Paused 状态发送消息写入账本但不广播
#[tokio::test]
async fn scenario5_paused_group_no_broadcast() {
    let (_dir, state, mut group) = setup().await;
    let group_id = &group.group_id;

    // 设置 Group 为 Paused
    set_group_state(&state.groups_dir, &mut group, GroupState::Paused)
        .expect("设置 Paused 失败");

    // 订阅广播通道
    let mut rx = state.event_tx.subscribe();

    send_message(
        &state, group_id, "claude",
        vec!["codex".to_string()],
        "Paused 状态消息".to_string(), None,
    )
    .await.expect("Paused 状态发送应成功");

    // 账本中应有 ChatMessage 事件
    let ledger_path = state.groups_dir
        .join(group_id)
        .join("state/ledger/ledger.jsonl");
    let chat_count = iter_events(&ledger_path).unwrap()
        .filter_map(|r| r.ok())
        .filter(|e| e.kind == EventKind::ChatMessage)
        .count();
    assert!(chat_count > 0, "Paused 状态消息应写入账本");

    // 不应广播 ChatMessage 事件
    assert!(
        rx.try_recv().is_err(),
        "Paused 状态不应广播 ChatMessage 事件"
    );
}
```

#### 测试 5-2: `scenario5_paused_inbox_still_queryable`

Paused 状态下写入账本的消息仍可通过 `unread_messages` 查询到（因为 inbox 基于账本，不依赖广播）。

```rust
/// 场景 5-2：Paused 状态写入账本的消息仍可通过 inbox 查询
#[tokio::test]
async fn scenario5_paused_inbox_still_queryable() {
    let (_dir, state, mut group) = setup().await;
    let group_id = &group.group_id;

    // 设置 Paused 并发送消息
    set_group_state(&state.groups_dir, &mut group, GroupState::Paused)
        .expect("设置 Paused 失败");

    send_message(
        &state, group_id, "claude",
        vec!["codex".to_string()],
        "Paused 时的消息".to_string(), None,
    )
    .await.expect("Paused 状态发送应成功");

    // codex 可以通过 unread_messages 查到（基于账本扫描，不依赖广播）
    let inbox = unread_messages(&state, group_id, "codex", 50).unwrap();
    assert_eq!(inbox.len(), 1, "Paused 时写入的消息可通过 inbox 查询");
}
```

#### 测试 5-3: `scenario5_active_group_broadcasts`

Active 状态下发送消息，event_tx 有广播事件。

```rust
/// 场景 5-3：Active 状态发送消息会触发 event_tx 广播
#[tokio::test]
async fn scenario5_active_group_broadcasts() {
    let (_dir, state, mut group) = setup().await;
    let group_id = &group.group_id;

    // 确保 Group 为 Active 状态
    set_group_state(&state.groups_dir, &mut group, GroupState::Active)
        .expect("设置 Active 失败");

    // 订阅广播通道
    let mut rx = state.event_tx.subscribe();

    send_message(
        &state, group_id, "claude",
        vec!["codex".to_string()],
        "Active 状态消息".to_string(), None,
    )
    .await.expect("Active 状态发送应成功");

    // event_tx 应收到 ChatMessage 广播
    let broadcast = rx.try_recv().expect("Active 状态应有广播事件");
    assert_eq!(broadcast.kind, EventKind::ChatMessage, "广播事件类型应为 ChatMessage");
}
```

#### 测试 5-4: `scenario5_paused_then_active_sequence`

完整的 Paused → Active 流程：先发一条（Paused，无广播），切换 Active，再发一条（有广播）。

```rust
/// 场景 5-4：Paused -> Active 状态转换后广播恢复
#[tokio::test]
async fn scenario5_paused_then_active_sequence() {
    let (_dir, state, mut group) = setup().await;
    let group_id = &group.group_id;

    // 第一步：Paused 状态发送（不广播）
    set_group_state(&state.groups_dir, &mut group, GroupState::Paused)
        .expect("设置 Paused 失败");

    let mut rx = state.event_tx.subscribe();

    send_message(
        &state, group_id, "claude",
        vec!["codex".to_string()],
        "Paused 消息（不广播）".to_string(), None,
    )
    .await.expect("发送失败");

    assert!(rx.try_recv().is_err(), "Paused 状态不应广播");

    // 第二步：切换为 Active 状态
    set_group_state(&state.groups_dir, &mut group, GroupState::Active)
        .expect("设置 Active 失败");

    send_message(
        &state, group_id, "claude",
        vec!["codex".to_string()],
        "Active 消息（广播）".to_string(), None,
    )
    .await.expect("发送失败");

    // 切换 Active 后发送的消息应广播
    let broadcast = rx.try_recv().expect("Active 状态应有广播事件");
    assert_eq!(broadcast.kind, EventKind::ChatMessage);
    assert_eq!(
        broadcast.data["text"].as_str().unwrap(),
        "Active 消息（广播）",
        "广播的内容应为 Active 状态发送的消息"
    );

    // 账本中应有 2 条 ChatMessage（Paused + Active 各 1 条）
    let ledger_path = state.groups_dir
        .join(group_id)
        .join("state/ledger/ledger.jsonl");
    let chat_count = iter_events(&ledger_path).unwrap()
        .filter_map(|r| r.ok())
        .filter(|e| e.kind == EventKind::ChatMessage)
        .count();
    assert_eq!(chat_count, 2, "账本应有 2 条 ChatMessage（Paused + Active 各 1 条）");
}
```

---

## 文件冲突检查

| 文件 | 状态 | 说明 |
|------|------|------|
| `crates/ghostcode-daemon/tests/integration_test.rs` | 新建 | T19 专属，无冲突 |
| `crates/ghostcode-daemon/Cargo.toml` | 无需修改 | `[dev-dependencies]` 已包含 `tempfile` + `proptest` |
| `Cargo.toml`（workspace） | 无需修改 | 所有依赖已在 workspace 中声明 |

现有测试文件（`send_test.rs`, `inbox_test.rs`, `lifecycle_test.rs` 等）完全不涉及，无冲突。

---

## 并行分组

所有子任务均为同一文件的不同测试函数，建议**顺序执行**：

| 阶段 | 内容 | 前提 |
|------|------|------|
| 第一步 | Task 1（setup 框架） | 无 |
| 第二步 | Task 2-6（5 个场景测试函数） | Task 1 完成 |

如果拆分给多人并行，可按场景分段：

```
Builder A: Task 1 + Task 2（场景 1）+ Task 3（场景 2）
Builder B: Task 4（场景 3）+ Task 5（场景 4）+ Task 6（场景 5）
```

由于都写入同一文件，并行时需注意合并。

---

## Builder 配置

### 单 Builder 模式（推荐）

```yaml
builder: codex
file: crates/ghostcode-daemon/tests/integration_test.rs
action: create
tasks:
  - task1_setup_framework
  - task2_scenario1_basic_message_flow
  - task3_scenario2_broadcast
  - task4_scenario3_persistence
  - task5_scenario4_actor_notify
  - task6_scenario5_group_state
```

### 验证命令

```bash
# 运行所有集成测试
cargo test --package ghostcode-daemon --test integration_test

# 运行指定场景
cargo test --package ghostcode-daemon --test integration_test scenario1
cargo test --package ghostcode-daemon --test integration_test scenario2
cargo test --package ghostcode-daemon --test integration_test scenario3
cargo test --package ghostcode-daemon --test integration_test scenario4
cargo test --package ghostcode-daemon --test integration_test scenario5
```

### 通过标准

- 全部 20 个测试（`#[tokio::test]`）均 PASS
- `cargo clippy --package ghostcode-daemon` 无 warning
- 无新增文件（仅 `integration_test.rs`）

---

## 完整文件结构预览

```
crates/ghostcode-daemon/tests/integration_test.rs
├── 文件头注释
├── use 导入（10 行）
├── make_actor() 辅助函数
├── setup() 辅助函数
├── // === 场景 1：基本消息流 ===
│   ├── scenario1_claude_sends_to_codex
│   ├── scenario1_codex_replies_to_claude
│   ├── scenario1_all_messages_in_ledger
│   └── scenario1_mark_read_clears_inbox
├── // === 场景 2：广播 ===
│   ├── scenario2_broadcast_reaches_codex_and_gemini
│   ├── scenario2_broadcast_excludes_sender
│   └── scenario2_broadcast_to_field_contains_both
├── // === 场景 3：持久化恢复 ===
│   ├── scenario3_inbox_survives_restart
│   ├── scenario3_ledger_survives_restart
│   └── scenario3_cursor_survives_restart
├── // === 场景 4：Agent 异常退出通知 ===
│   ├── scenario4_system_notify_persisted_in_ledger
│   ├── scenario4_system_notify_by_system
│   └── scenario4_actor_remove_recorded_in_ledger
└── // === 场景 5：Group 状态影响投递 ===
    ├── scenario5_paused_group_no_broadcast
    ├── scenario5_paused_inbox_still_queryable
    ├── scenario5_active_group_broadcasts
    └── scenario5_paused_then_active_sequence
```

**合计**: 20 个 `#[tokio::test]`，覆盖 T19 规格全部 5 个场景。
