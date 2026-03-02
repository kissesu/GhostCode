# Team Plan: T11 消息发送 + 事件写入

## 概述
实现 GhostCode Phase 1 消息系统的核心功能：`send_message` 和 `reply_message`，包含消息验证、Blob 溢出处理、账本写入和事件广播。

## Codex 分析摘要
Codex CLI 执行失败（status 1），与 #S1306 中记录的 Codex 稳定性问题一致。分析由 Claude 自行补充。

## Gemini 分析摘要
- 建议定义 `ChatMessageData` 结构化类型，保持与 CCCC 协议兼容
- 模块组织：`messaging/` 子目录，含 `mod.rs` + `send.rs`
- 原子性保障：先写账本，再广播事件
- 风险点：并发写入锁竞争（已有 flock 保护）、无效 reply_to 校验、不存在的收件人处理

## 技术方案

### 架构决策

1. **模块位置**: `crates/ghostcode-daemon/src/messaging/mod.rs` + `send.rs`
   - 与 CCCC 的 `daemon/messaging/` 目录结构一致
   - 为 T12(inbox) 和 T13(delivery) 预留子模块位置

2. **数据结构**: 不定义独立 ChatMessageData struct
   - 保持与现有代码一致：Event.data 使用 `serde_json::Value`
   - Phase 1 聚焦核心功能，避免过度抽象
   - 参考: 现有 lifecycle.rs 中 event.data 使用 `serde_json::json!({...})` 构造

3. **错误处理**: `MessagingError` 枚举（thiserror derive）
   - 参考: runner.rs 中 `LifecycleError` 的模式

4. **Event.data 字段设计**（ChatMessage 事件）:
   ```json
   {
     "text": "消息正文",
     "to": ["codex", "gemini"],
     "reply_to": null,
     "quote_text": null,
     "sender_id": "claude"
   }
   ```
   - blob 溢出时 text 被替换为 `{"_blob_ref": "chat.<event_id>.txt", "body_preview": "前200字符..."}`

5. **收件人解析**: recipients 为空时广播给 Group 内除 sender 外所有 Actor
   - 参考: cccc/src/cccc/daemon/messaging/chat_ops.py handle_send

6. **Group Paused 处理**: 消息写入账本但不通过 event_tx 广播
   - 参考: phase1-tasks.md T11 约束

### 关键集成点

| 依赖模块 | API | 用途 |
|----------|-----|------|
| `ghostcode-ledger` | `append_event(ledger_path, lock_path, event)` | 持久化消息事件 |
| `ghostcode-ledger::blob` | `maybe_spill_blob(blobs_dir, event_id, kind, data)` | 32KB+ 消息溢出 |
| `crate::group` | `load_group(group_dir)` | 加载 Group 信息验证 |
| `crate::actor_mgmt` | `find_actor(group, actor_id)`, `list_actors(group)` | 验证 sender/recipients |
| `crate::server::AppState` | `groups_dir`, `event_tx` | 路径 + 事件广播 |
| `ghostcode-types::event` | `Event::new(kind, group_id, scope_key, by, data)` | 构造事件 |
| `ghostcode-ledger` | `iter_events(ledger_path)` | reply 时查找原始事件 |

### 账本路径（固定格式）
```rust
let group_dir = state.groups_dir.join(group_id);
let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
let lock_path = group_dir.join("state/ledger/ledger.lock");
let blobs_dir = group_dir.join("state/ledger/blobs");
```

## 子任务列表

### Task 1: 创建 messaging 模块 + send/reply 核心逻辑
- **类型**: 后端
- **文件范围**:
  - `crates/ghostcode-daemon/src/messaging/mod.rs` (新建)
  - `crates/ghostcode-daemon/src/messaging/send.rs` (新建)
  - `crates/ghostcode-daemon/src/lib.rs` (添加 `pub mod messaging;`)
- **依赖**: 无
- **实施步骤**:

  1. 创建 `crates/ghostcode-daemon/src/messaging/` 目录

  2. 创建 `crates/ghostcode-daemon/src/messaging/mod.rs`:
     ```rust
     //! 消息投递模块
     //!
     //! Phase 1 阶段 E 的核心模块，处理消息发送、接收和投递
     //! - send.rs: 消息发送 + 事件写入（T11）
     //! - inbox.rs: 收件箱读取 + 已读游标（T12，待实现）
     //! - delivery.rs: 投递引擎（T13，待实现）
     //!
     //! 参考: cccc/src/cccc/daemon/messaging/ - 消息系统完整实现
     //!
     //! @author Atlas.oi
     //! @date 2026-03-01

     pub mod send;
     ```

  3. 创建 `crates/ghostcode-daemon/src/messaging/send.rs`，实现以下内容:

     **文件头注释**: 标准格式，说明消息发送核心逻辑，参考 cccc/src/cccc/daemon/messaging/chat_ops.py

     **错误类型**:
     ```rust
     #[derive(Debug, thiserror::Error)]
     pub enum MessagingError {
         #[error("IO error: {0}")]
         Io(#[from] std::io::Error),
         #[error("Ledger error: {0}")]
         Ledger(#[from] ghostcode_ledger::LedgerError),
         #[error("YAML error: {0}")]
         Yaml(#[from] serde_yaml::Error),
         #[error("Group not found: {0}")]
         GroupNotFound(String),
         #[error("Sender not found: actor '{actor_id}' not in group '{group_id}'")]
         SenderNotFound { group_id: String, actor_id: String },
         #[error("Event not found: {0}")]
         EventNotFound(String),
         #[error("Invalid recipient: actor '{0}' not in group")]
         InvalidRecipient(String),
     }

     pub type Result<T> = std::result::Result<T, MessagingError>;
     ```

     **send_message 函数**:
     ```rust
     /// 发送消息（写入 ChatMessage 事件到账本 + 广播通知）
     ///
     /// 业务逻辑：
     /// 1. 加载 Group，验证 sender 是已注册 Actor
     /// 2. 解析收件人：recipients 为空时广播给除 sender 外所有 Actor
     /// 3. 验证所有收件人在 Group 内存在（不存在的跳过并记录 warn 日志）
     /// 4. 构造 Event.data JSON（包含 text, to, reply_to, sender_id）
     /// 5. 调用 maybe_spill_blob 处理 body > 32KB 的溢出
     /// 6. 调用 append_event 写入账本
     /// 7. 检查 Group 状态：非 Paused 时通过 event_tx 广播
     /// 8. 返回写入的 Event
     ///
     /// 参考: cccc/src/cccc/daemon/messaging/chat_ops.py handle_send
     pub async fn send_message(
         state: &AppState,
         group_id: &str,
         sender_id: &str,
         recipients: Vec<String>,
         body: String,
         reply_to: Option<String>,
     ) -> Result<Event>
     ```

     **send_message 实现细节**:
     - 路径构造：`state.groups_dir.join(group_id)` → group_dir, ledger_path, lock_path, blobs_dir
     - 加载 Group: `crate::group::load_group(&group_dir)` → map_err GroupNotFound
     - 验证 sender: `crate::actor_mgmt::find_actor(&group, sender_id)` → None 则 SenderNotFound
     - 收件人解析:
       ```rust
       let resolved_recipients = if recipients.is_empty() {
           // 广播：Group 内除 sender 外所有 Actor 的 actor_id
           crate::actor_mgmt::list_actors(&group)
               .iter()
               .filter(|a| a.actor_id != sender_id)
               .map(|a| a.actor_id.clone())
               .collect()
       } else {
           // 指定收件人：过滤不存在的并记录 warn
           let mut valid = Vec::new();
           for r in &recipients {
               if crate::actor_mgmt::find_actor(&group, r).is_some() {
                   valid.push(r.clone());
               } else {
                   tracing::warn!("recipient '{}' not found in group '{}', skipping", r, group_id);
               }
           }
           valid
       };
       ```
     - 构造 Event data JSON:
       ```rust
       let mut data = serde_json::json!({
           "text": body,
           "to": resolved_recipients,
           "reply_to": reply_to,
           "sender_id": sender_id,
       });
       ```
     - Blob 溢出检查（在创建 Event 之前）:
       ```rust
       // 先创建临时 Event 获取 id（用于 blob 文件名）
       let event_id = uuid::Uuid::new_v4().simple().to_string();
       let data = ghostcode_ledger::blob::maybe_spill_blob(
           &blobs_dir,
           &event_id,
           &EventKind::ChatMessage,
           &data,
       )?;
       ```
       注意: maybe_spill_blob 仅对 ChatMessage 类型检查，内部判断 data["text"] 长度
     - 构造 Event（使用预生成的 event_id）:
       ```rust
       // 注意：不能用 Event::new（它内部会生成新 id），需要手动构造
       // 或者修改调用方式，先生成 Event 再做 blob 处理
       // 实际实现方案：先构造完整 Event，然后对 event.data 做 blob 处理
       let mut event = Event::new(
           EventKind::ChatMessage,
           group_id,
           "default",  // scope_key Phase 1 固定空串（按 AMB-2 决策）
           sender_id,
           data,
       );
       // 对 event.data 做 blob 溢出处理
       event.data = ghostcode_ledger::blob::maybe_spill_blob(
           &blobs_dir,
           &event.id,
           &event.kind,
           &event.data,
       )?;
       ```
     - 写入账本:
       ```rust
       ghostcode_ledger::append_event(&ledger_path, &lock_path, &event)?;
       ```
     - 广播通知（检查 Group 状态）:
       ```rust
       use ghostcode_types::group::GroupState;
       if group.state != GroupState::Paused {
           // 广播失败不影响返回（可能无订阅者）
           let _ = state.event_tx.send(event.clone());
       }
       ```
     - 返回 `Ok(event)`

     **reply_message 函数**:
     ```rust
     /// 回复消息（send_message 的便捷封装）
     ///
     /// 业务逻辑：
     /// 1. 从账本查找原始事件（reply_to_event_id）
     /// 2. 提取原始发送者作为默认收件人
     /// 3. 提取原始文本前 100 字符作为 quote_text
     /// 4. 调用 send_message 完成实际发送
     ///
     /// 参考: cccc/src/cccc/daemon/messaging/chat_ops.py handle_reply
     pub async fn reply_message(
         state: &AppState,
         group_id: &str,
         sender_id: &str,
         reply_to_event_id: &str,
         body: String,
     ) -> Result<Event>
     ```

     **reply_message 实现细节**:
     - 构造路径: group_dir, ledger_path
     - 查找原始事件:
       ```rust
       let ledger_path = state.groups_dir.join(group_id).join("state/ledger/ledger.jsonl");
       let original_event = ghostcode_ledger::iter_events(&ledger_path)?
           .filter_map(|r| r.ok())
           .find(|e| e.id == reply_to_event_id)
           .ok_or_else(|| MessagingError::EventNotFound(reply_to_event_id.to_string()))?;
       ```
     - 提取 quote_text（原始文本前 100 字符）:
       ```rust
       let quote_text = original_event.data.get("text")
           .and_then(|v| v.as_str())
           .map(|t| {
               if t.len() > 100 { format!("{}...", &t[..100]) } else { t.to_string() }
           });
       ```
       注意: 中文字符截断需要用 `.chars().take(100)` 而非字节切片
       ```rust
       let quote_text = original_event.data.get("text")
           .and_then(|v| v.as_str())
           .map(|t| {
               let chars: String = t.chars().take(100).collect();
               if t.chars().count() > 100 { format!("{}...", chars) } else { chars }
           });
       ```
     - 推断默认收件人（原始消息的发送者）:
       ```rust
       let original_sender = original_event.by.clone();
       let recipients = vec![original_sender];
       ```
     - 构造带 quote_text 的 body 或传入 reply_to:
       实际上 send_message 已接受 reply_to 参数，所以直接调用:
       ```rust
       // 直接复用 send_message，传入 reply_to
       send_message(
           state,
           group_id,
           sender_id,
           recipients,
           body,
           Some(reply_to_event_id.to_string()),
       ).await
       ```
       但需要在 Event.data 中额外加入 quote_text。

       **更好的方案**: 在 send_message 内部不处理 quote_text，reply_message 在调用 send_message 后修改返回的 Event 的 data。但这不行因为 Event 已写入账本。

       **最终方案**: reply_message 不调用 send_message，而是自己实现完整流程（与 send_message 逻辑相似但 data 中多 quote_text 字段）。或者抽取内部公共函数。

       **推荐实现**: 抽取内部 `do_send` 函数，接受完整的 `serde_json::Value` data:
       ```rust
       /// 内部发送实现（公共逻辑）
       async fn do_send(
           state: &AppState,
           group_id: &str,
           sender_id: &str,
           data: serde_json::Value,
       ) -> Result<Event>
       ```
       - send_message 构造 data（含 text, to, reply_to, sender_id）→ 调用 do_send
       - reply_message 构造 data（额外含 quote_text）→ 调用 do_send
       - do_send 内部做：加载 group → 验证 sender → blob 处理 → 构造 Event → append → broadcast

  4. 在 `crates/ghostcode-daemon/src/lib.rs` 中添加 `pub mod messaging;`（在 `pub mod lifecycle;` 之后）

- **验收标准**:
  - `cargo check -p ghostcode-daemon` 零错误
  - `send_message` 和 `reply_message` 函数已导出
  - 内部 `do_send` 逻辑完整（验证 → blob → 账本 → 广播）

### Task 2: 连接 dispatch handler
- **类型**: 后端
- **文件范围**:
  - `crates/ghostcode-daemon/src/dispatch.rs` (修改)
- **依赖**: Task 1
- **实施步骤**:

  1. 在 dispatch.rs 顶部添加导入:
     ```rust
     use crate::messaging::send;
     ```

  2. 将 dispatch 函数中的 `"send"` stub 替换为实际 handler:
     ```rust
     "send" => handle_send(state, &req.args).await,
     ```

  3. 将 `"reply"` stub 替换为:
     ```rust
     "reply" => handle_reply(state, &req.args).await,
     ```

  4. 实现 `handle_send` handler（参考现有 `handle_actor_start` 的模式）:
     ```rust
     /// send handler
     ///
     /// 发送消息到指定收件人或广播
     ///
     /// 必填参数：group_id, sender_id (或 by), body (或 text)
     /// 可选参数：to (收件人列表，空=广播), reply_to (回复的 event_id)
     async fn handle_send(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
         // 提取 group_id（必填）
         let group_id = match args["group_id"].as_str() {
             Some(v) => v,
             None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
         };
         // 提取 sender_id（必填，兼容 "by" 字段名）
         let sender_id = match args["sender_id"].as_str().or_else(|| args["by"].as_str()) {
             Some(v) => v,
             None => return DaemonResponse::err("INVALID_ARGS", "missing required field: sender_id"),
         };
         // 提取 body（必填，兼容 "text" 字段名）
         let body = match args["body"].as_str().or_else(|| args["text"].as_str()) {
             Some(v) => v.to_string(),
             None => return DaemonResponse::err("INVALID_ARGS", "missing required field: body"),
         };
         // 提取 to（可选，默认空=广播）
         let recipients: Vec<String> = args["to"]
             .as_array()
             .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
             .unwrap_or_default();
         // 提取 reply_to（可选）
         let reply_to = args["reply_to"].as_str().map(String::from);

         match send::send_message(state, group_id, sender_id, recipients, body, reply_to).await {
             Ok(event) => DaemonResponse::ok(serde_json::json!({ "event": serde_json::to_value(&event).unwrap_or_default() })),
             Err(e) => DaemonResponse::err("MESSAGING_ERROR", e.to_string()),
         }
     }
     ```

  5. 实现 `handle_reply` handler:
     ```rust
     /// reply handler
     ///
     /// 回复指定消息
     ///
     /// 必填参数：group_id, sender_id (或 by), reply_to, body (或 text)
     async fn handle_reply(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
         let group_id = match args["group_id"].as_str() {
             Some(v) => v,
             None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
         };
         let sender_id = match args["sender_id"].as_str().or_else(|| args["by"].as_str()) {
             Some(v) => v,
             None => return DaemonResponse::err("INVALID_ARGS", "missing required field: sender_id"),
         };
         let reply_to = match args["reply_to"].as_str() {
             Some(v) => v,
             None => return DaemonResponse::err("INVALID_ARGS", "missing required field: reply_to"),
         };
         let body = match args["body"].as_str().or_else(|| args["text"].as_str()) {
             Some(v) => v.to_string(),
             None => return DaemonResponse::err("INVALID_ARGS", "missing required field: body"),
         };

         match send::reply_message(state, group_id, sender_id, reply_to, body).await {
             Ok(event) => DaemonResponse::ok(serde_json::json!({ "event": serde_json::to_value(&event).unwrap_or_default() })),
             Err(e) => DaemonResponse::err("MESSAGING_ERROR", e.to_string()),
         }
     }
     ```

- **验收标准**:
  - `cargo check -p ghostcode-daemon` 零错误
  - dispatch("send", ...) 不再返回 NOT_IMPLEMENTED
  - dispatch("reply", ...) 不再返回 NOT_IMPLEMENTED

### Task 3: TDD 测试套件
- **类型**: 后端测试
- **文件范围**:
  - `crates/ghostcode-daemon/tests/send_test.rs` (新建)
- **依赖**: Task 1
- **实施步骤**:

  1. 创建 `crates/ghostcode-daemon/tests/send_test.rs`

  2. 文件头注释（标准格式）

  3. 导入（参考现有 lifecycle_test.rs 的模式）:
     ```rust
     use std::sync::Arc;
     use ghostcode_daemon::actor_mgmt::add_actor;
     use ghostcode_daemon::group::{create_group, load_group, set_group_state};
     use ghostcode_daemon::messaging::send::{send_message, reply_message};
     use ghostcode_daemon::server::AppState;
     use ghostcode_types::actor::{ActorInfo, ActorRole, RuntimeKind};
     use ghostcode_types::event::EventKind;
     use ghostcode_types::group::GroupState;
     use ghostcode_ledger::{iter_events, count_events};
     use tempfile::TempDir;
     ```

  4. setup 辅助函数（与 lifecycle_test.rs 一致的模式）:
     ```rust
     fn make_actor(actor_id: &str, role: ActorRole, runtime: RuntimeKind) -> ActorInfo { ... }

     /// 创建测试环境：TempDir + AppState + Group（含 3 个 Actor）
     /// 返回 (TempDir, Arc<AppState>, GroupInfo)
     /// Actors: claude(Foreman), codex(Peer), gemini(Peer)
     async fn setup() -> (TempDir, Arc<AppState>, ghostcode_types::group::GroupInfo) { ... }
     ```
     - 完全复用 lifecycle_test.rs 中的 setup 模式
     - TempDir → groups_dir → AppState::new → create_group → add 3 actors

  5. 实现 6 个测试用例:

     **#[tokio::test] async fn send_message_persisted()**
     - setup 环境
     - 调用 send_message(state, group_id, "claude", vec!["codex"], "hello codex", None)
     - 构造 ledger_path = groups_dir.join(group_id).join("state/ledger/ledger.jsonl")
     - 调用 iter_events(&ledger_path)，过滤 kind == ChatMessage
     - assert 找到 1 条，且 data["text"] == "hello codex"

     **#[tokio::test] async fn send_to_specific_recipient()**
     - setup 环境
     - claude → codex 发送消息（recipients = vec!["codex"]）
     - 验证事件 data["to"] 包含 "codex" 但不包含 "gemini"

     **#[tokio::test] async fn broadcast_to_all()**
     - setup 环境
     - claude 广播（recipients = vec![]）
     - 验证事件 data["to"] 包含 "codex" 和 "gemini"，不包含 "claude"

     **#[tokio::test] async fn reply_links_to_original()**
     - setup 环境
     - claude 先发送原始消息 → 获取 original_event
     - codex 回复 reply_message(state, group_id, "codex", original_event.id, "got it")
     - 验证回复事件 data["reply_to"] == original_event.id
     - 验证回复事件 data["quote_text"] 存在且包含原始消息文本

     **#[tokio::test] async fn large_message_blob_spill()**
     - setup 环境
     - 构造 50KB 文本：`"x".repeat(50 * 1024)`
     - 发送大消息
     - 验证 blobs_dir 下存在 `chat.<event_id>.txt` 文件
     - 验证 Event.data 中包含 `_blob_ref` 字段
     - 验证 blob 文件内容 == 原始 50KB 文本

     **#[tokio::test] async fn paused_group_no_delivery()**
     - setup 环境
     - 调用 set_group_state(&groups_dir, &mut group, GroupState::Paused)
     - 订阅 event_tx: `let mut rx = state.event_tx.subscribe()`
     - 发送消息
     - 验证账本中有该消息（iter_events 找到）
     - 验证 rx.try_recv() 返回 Err（无广播事件）

  6. 额外边界测试:

     **#[tokio::test] async fn send_invalid_sender_rejected()**
     - setup 环境
     - 用不存在的 sender_id "unknown" 发送
     - 验证返回 Err(SenderNotFound)

     **#[tokio::test] async fn reply_invalid_event_rejected()**
     - setup 环境
     - reply 一个不存在的 event_id
     - 验证返回 Err(EventNotFound)

- **验收标准**:
  - `cargo test -p ghostcode-daemon --test send_test` 全部通过
  - 8 个测试用例覆盖核心场景 + 边界情况

## 文件冲突检查
- Task 1 修改: messaging/mod.rs(新), messaging/send.rs(新), lib.rs(仅添加一行)
- Task 2 修改: dispatch.rs
- Task 3 修改: tests/send_test.rs(新)
- lib.rs 由 Task 1 独占修改（添加 `pub mod messaging;`）
- **无文件冲突**

## 并行分组
- **Layer 1** (独立): Task 1 — 核心模块实现
- **Layer 2** (并行，依赖 Layer 1): Task 2 + Task 3 — dispatch 接入 + 测试

## Builder 配置
- Builder 数量: 3 个（Sonnet）
- Layer 1: 1 个 Builder
- Layer 2: 2 个 Builder 并行
