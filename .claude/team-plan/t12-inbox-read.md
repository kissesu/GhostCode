# Team Plan: T12 Inbox 读取 + 已读游标

## 概述
实现 GhostCode Phase 1 消息系统的 Inbox 功能：未读消息查询、已读游标管理、消息 ACK，完成消息系统的"读"端闭环。

## Codex 分析摘要
Codex CLI 执行失败（status 1），与 T11 中记录的 Codex 稳定性问题一致。分析由 Claude 自行补充。

## Gemini 分析摘要
- 架构模式：Event Sourcing + Projection（read_cursors.json 是账本状态的物化视图）
- 建议拆分 cursor.rs（游标持久化）和 inbox.rs（业务逻辑）两层
- 一致性策略：先写 ChatRead 事件到账本，再更新 read_cursors.json
- 游标单调性：通过 ts 字符串字典序比较（ISO 8601 特性）
- 性能建议：unread_count 只计数不返回完整事件

## 技术方案

### 架构决策

1. **模块位置**: `crates/ghostcode-daemon/src/messaging/inbox.rs`（单文件）
   - Gemini 建议拆 cursor.rs + inbox.rs，但 Phase 1 游标逻辑简单（~30 行），不值得单独一个文件
   - 保持与 T11 send.rs 同级的扁平结构
   - 如果 Phase 2 需要更复杂的游标管理，再提取

2. **错误类型**: 复用 `send.rs` 中的 `MessagingError`
   - inbox 需要的错误变体（GroupNotFound, EventNotFound, Io, Ledger）已在 MessagingError 中定义
   - 新增 `CursorRegressed` 变体（游标回退时报错）—— 实际上按 CCCC 参考实现，游标回退时静默忽略（不是错误），所以不需要新变体
   - 将 `MessagingError` 和 `Result` 类型从 `send.rs` 移动到 `mod.rs`，让 inbox.rs 也能使用

3. **游标比较策略**: 基于 `ts` 字符串比较（ISO 8601 字典序）
   - 参考: cccc/src/cccc/kernel/inbox.py:62-75 set_cursor 中用 parse_utc_iso 比较
   - GhostCode 的 ts 格式：`2026-03-01T12:00:00.000000Z`（固定格式），字符串比较等价于时间比较
   - 不需要解析为 DateTime，直接字符串 `>` 比较即可（KISS 原则）

4. **read_cursors.json 读写**:
   - 用 serde_json 读写
   - 用 flock 保护写操作（复用 ghostcode-ledger 的 with_lock 或直接用 fs2）
   - 游标文件路径：`<group_dir>/state/read_cursors.json`
   - 锁文件路径：`<group_dir>/state/read_cursors.lock`

5. **mark_read 双写策略**:
   - 第一步：写入 ChatRead 事件到账本（不可变记录）
   - 第二步：更新 read_cursors.json（快速查询的物化视图）
   - 参考: cccc/src/cccc/kernel/inbox.py:62-83 set_cursor

6. **unread_messages 过滤逻辑**:
   - 遍历账本所有事件
   - 过滤 kind == ChatMessage
   - 排除 sender == actor_id（自己发的不算）
   - 检查 data["to"] 包含 actor_id（或 to 为空表示广播）
   - 过滤 ts > cursor_ts
   - 参考: cccc/src/cccc/kernel/inbox.py:450-492 unread_messages

### 关键集成点

| 依赖模块 | API | 用途 |
|----------|-----|------|
| `ghostcode-ledger` | `append_event(ledger_path, lock_path, event)` | mark_read 写入 ChatRead 事件 |
| `ghostcode-ledger` | `iter_events(ledger_path)` | 遍历账本查找未读消息 |
| `crate::group` | `load_group(group_dir)` | 加载 Group 信息 |
| `crate::messaging::send` | `MessagingError`, `Result` | 复用错误类型 |
| `ghostcode-types::event` | `Event::new`, `EventKind::ChatRead/ChatAck` | 构造事件 |
| `crate::server::AppState` | `groups_dir`, `event_tx` | 路径 + 事件广播 |

### 路径（固定格式）
```rust
let group_dir = state.groups_dir.join(group_id);
let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
let lock_path = group_dir.join("state/ledger/ledger.lock");
let cursors_path = group_dir.join("state/read_cursors.json");
let cursors_lock_path = group_dir.join("state/read_cursors.lock");
```

## 子任务列表

### Task 1: 重构 MessagingError 到 mod.rs + 实现 inbox.rs 核心逻辑
- **类型**: 后端
- **文件范围**:
  - `crates/ghostcode-daemon/src/messaging/mod.rs` (修改：移入 MessagingError + 添加 pub mod inbox)
  - `crates/ghostcode-daemon/src/messaging/send.rs` (修改：删除 MessagingError 定义，改为 use super::)
  - `crates/ghostcode-daemon/src/messaging/inbox.rs` (新建)
- **依赖**: 无
- **实施步骤**:

  1. **重构 MessagingError 到 mod.rs**:
     - 将 `send.rs` 中的 `MessagingError` 枚举和 `pub type Result<T>` 移动到 `mod.rs`
     - 在 `mod.rs` 顶部添加必要的 imports:
       ```rust
       use ghostcode_ledger::LedgerError;
       ```
     - 在 `send.rs` 中删除 MessagingError 定义，替换为:
       ```rust
       use super::{MessagingError, Result};
       ```
     - 确保 send.rs 中的所有 MessagingError 引用仍然正常工作

  2. **在 mod.rs 中添加 inbox 模块声明**:
     ```rust
     pub mod inbox;
     ```

  3. **创建 `crates/ghostcode-daemon/src/messaging/inbox.rs`**，实现以下内容:

     **文件头注释**: 标准格式，说明 Inbox 读取 + 已读游标核心逻辑，参考 cccc/src/cccc/kernel/inbox.py

     **游标读写函数**:

     ```rust
     use std::collections::HashMap;
     use std::path::Path;

     use serde::{Deserialize, Serialize};
     use ghostcode_types::event::{Event, EventKind};

     use crate::server::AppState;
     use super::Result;
     use super::MessagingError;

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
     ```

     **load_cursors 函数**:
     ```rust
     /// 从磁盘加载已读游标
     /// 文件不存在时返回空 HashMap（首次使用场景）
     ///
     /// 参考: cccc/src/cccc/kernel/inbox.py:38-42 load_cursors
     fn load_cursors(cursors_path: &Path) -> Cursors {
         if !cursors_path.exists() {
             return HashMap::new();
         }
         match std::fs::read_to_string(cursors_path) {
             Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
             Err(_) => HashMap::new(),
         }
     }
     ```

     **save_cursors 函数**:
     ```rust
     /// 持久化已读游标到磁盘
     /// 使用 flock 保护并发写入
     ///
     /// 参考: cccc/src/cccc/kernel/inbox.py:45-48 _save_cursors
     fn save_cursors(cursors_path: &Path, lock_path: &Path, cursors: &Cursors) -> Result<()> {
         // 确保父目录存在
         if let Some(parent) = cursors_path.parent() {
             std::fs::create_dir_all(parent)?;
         }
         ghostcode_ledger::with_lock(lock_path, || {
             let json = serde_json::to_string_pretty(cursors)
                 .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
             std::fs::write(cursors_path, json)?;
             Ok(())
         })?;
         Ok(())
     }
     ```
     注意：ghostcode_ledger::with_lock 是 private 函数（已验证），需直接使用 fs2 的 flock:
     ```rust
     use fs2::FileExt;
     ```
     fs2 已在 Cargo.toml 的依赖中（ghostcode-ledger 使用）。ghostcode-daemon 的 Cargo.toml 需要添加 `fs2` 依赖。

     **is_message_for_actor 函数**（内部辅助）:
     ```rust
     /// 判断消息是否应该投递给指定 Actor
     /// 检查 data["to"] 列表：包含 actor_id 或为空（广播模式）
     ///
     /// 参考: cccc/src/cccc/kernel/inbox.py:395-447 is_message_for_actor
     fn is_message_for_actor(event: &Event, actor_id: &str) -> bool {
         // 只处理 ChatMessage
         if event.kind != EventKind::ChatMessage {
             return false;
         }
         // 排除自己发送的消息
         if event.by == actor_id {
             return false;
         }
         // 检查收件人列表
         match event.data.get("to").and_then(|v| v.as_array()) {
             Some(recipients) => {
                 // 空数组 = 广播
                 if recipients.is_empty() {
                     return true;
                 }
                 // 检查是否包含 actor_id 或 @all
                 recipients.iter().any(|r| {
                     r.as_str().map_or(false, |s| s == actor_id || s == "@all")
                 })
             }
             // 没有 to 字段 = 广播
             None => true,
         }
     }
     ```

     **unread_messages 函数**:
     ```rust
     /// 获取 Actor 的未读消息列表
     ///
     /// 业务逻辑：
     /// 1. 加载已读游标获取 cursor_ts
     /// 2. 遍历账本，过滤 ChatMessage 且 ts > cursor_ts 且收件人匹配
     /// 3. 按时间正序返回，限制数量
     ///
     /// 参考: cccc/src/cccc/kernel/inbox.py:450-492 unread_messages
     ///
     /// @param state - 共享应用状态
     /// @param group_id - Group ID
     /// @param actor_id - 查询未读的 Actor ID
     /// @param limit - 最大返回数量
     /// @return 未读消息事件列表（时间正序）
     pub fn unread_messages(
         state: &AppState,
         group_id: &str,
         actor_id: &str,
         limit: usize,
     ) -> Result<Vec<Event>> {
         let group_dir = state.groups_dir.join(group_id);
         let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
         let cursors_path = group_dir.join("state/read_cursors.json");

         // 验证 Group 存在
         crate::group::load_group(&group_dir)
             .map_err(|_| MessagingError::GroupNotFound(group_id.to_string()))?;

         // 加载游标
         let cursors = load_cursors(&cursors_path);
         let cursor_ts = cursors.get(actor_id).map(|c| c.ts.as_str());

         // 如果账本不存在，返回空
         if !ledger_path.exists() {
             return Ok(Vec::new());
         }

         // 遍历账本，收集未读消息
         let mut unread = Vec::new();
         for event_result in ghostcode_ledger::iter_events(&ledger_path)? {
             let event = match event_result {
                 Ok(e) => e,
                 Err(_) => continue,
             };

             // 只处理 ChatMessage
             if event.kind != EventKind::ChatMessage {
                 continue;
             }

             // 检查是否属于该 Actor 的消息
             if !is_message_for_actor(&event, actor_id) {
                 continue;
             }

             // 检查是否在游标之后（未读）
             if let Some(cts) = cursor_ts {
                 if event.ts.as_str() <= cts {
                     continue;
                 }
             }

             unread.push(event);

             // 限制数量
             if limit > 0 && unread.len() >= limit {
                 break;
             }
         }

         Ok(unread)
     }
     ```

     **unread_count 函数**:
     ```rust
     /// 获取未读消息数（轻量版，只计数不返回事件）
     ///
     /// 参考: cccc/src/cccc/kernel/inbox.py:495-528 unread_count
     pub fn unread_count(
         state: &AppState,
         group_id: &str,
         actor_id: &str,
     ) -> Result<usize> {
         let group_dir = state.groups_dir.join(group_id);
         let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
         let cursors_path = group_dir.join("state/read_cursors.json");

         crate::group::load_group(&group_dir)
             .map_err(|_| MessagingError::GroupNotFound(group_id.to_string()))?;

         let cursors = load_cursors(&cursors_path);
         let cursor_ts = cursors.get(actor_id).map(|c| c.ts.as_str());

         if !ledger_path.exists() {
             return Ok(0);
         }

         let mut count = 0usize;
         for event_result in ghostcode_ledger::iter_events(&ledger_path)? {
             let event = match event_result {
                 Ok(e) => e,
                 Err(_) => continue,
             };
             if event.kind != EventKind::ChatMessage {
                 continue;
             }
             if !is_message_for_actor(&event, actor_id) {
                 continue;
             }
             if let Some(cts) = cursor_ts {
                 if event.ts.as_str() <= cts {
                     continue;
                 }
             }
             count += 1;
         }

         Ok(count)
     }
     ```

     **mark_read 函数**:
     ```rust
     /// 标记已读（移动游标到指定 event_id）
     ///
     /// 业务逻辑：
     /// 1. 从账本查找目标事件，获取其 ts
     /// 2. 检查游标单调性：新 ts 必须 >= 当前游标 ts
     /// 3. 写入 ChatRead 事件到账本
     /// 4. 更新 read_cursors.json
     ///
     /// 参考: cccc/src/cccc/kernel/inbox.py:62-83 set_cursor
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

         // 验证 Group 存在
         crate::group::load_group(&group_dir)
             .map_err(|_| MessagingError::GroupNotFound(group_id.to_string()))?;

         // 从账本查找目标事件
         let target_event = ghostcode_ledger::iter_events(&ledger_path)?
             .filter_map(|r| r.ok())
             .find(|e| e.id == event_id)
             .ok_or_else(|| MessagingError::EventNotFound(event_id.to_string()))?;

         // 加载当前游标
         let mut cursors = load_cursors(&cursors_path);

         // 单调性检查：如果新事件 ts <= 当前游标 ts，静默忽略（参考 CCCC 行为）
         if let Some(current) = cursors.get(actor_id) {
             if target_event.ts <= current.ts {
                 return Ok(());
             }
         }

         // 写入 ChatRead 事件到账本
         let read_event = Event::new(
             EventKind::ChatRead,
             group_id,
             "",  // scope_key Phase 1 固定空串
             actor_id,
             serde_json::json!({
                 "event_id": event_id,
                 "actor_id": actor_id,
             }),
         );
         ghostcode_ledger::append_event(&ledger_path, &lock_path, &read_event)?;

         // 更新游标
         cursors.insert(actor_id.to_string(), CursorEntry {
             event_id: event_id.to_string(),
             ts: target_event.ts.clone(),
         });
         save_cursors(&cursors_path, &cursors_lock_path, &cursors)?;

         Ok(())
     }
     ```

     **mark_all_read 函数**:
     ```rust
     /// 全部标记已读
     ///
     /// 业务逻辑：
     /// 1. 找到账本中最后一条属于该 Actor 的未读消息
     /// 2. 如果有未读，调用 mark_read 标记到最后一条
     /// 3. 如果无未读，静默成功
     ///
     /// 参考: cccc/src/cccc/kernel/inbox.py:605-641 latest_unread_event
     pub fn mark_all_read(
         state: &AppState,
         group_id: &str,
         actor_id: &str,
     ) -> Result<()> {
         let group_dir = state.groups_dir.join(group_id);
         let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
         let cursors_path = group_dir.join("state/read_cursors.json");

         crate::group::load_group(&group_dir)
             .map_err(|_| MessagingError::GroupNotFound(group_id.to_string()))?;

         let cursors = load_cursors(&cursors_path);
         let cursor_ts = cursors.get(actor_id).map(|c| c.ts.as_str());

         if !ledger_path.exists() {
             return Ok(());
         }

         // 找最后一条未读消息
         let mut last_unread: Option<Event> = None;
         for event_result in ghostcode_ledger::iter_events(&ledger_path)? {
             let event = match event_result {
                 Ok(e) => e,
                 Err(_) => continue,
             };
             if event.kind != EventKind::ChatMessage {
                 continue;
             }
             if !is_message_for_actor(&event, actor_id) {
                 continue;
             }
             if let Some(cts) = cursor_ts {
                 if event.ts.as_str() <= cts {
                     continue;
                 }
             }
             last_unread = Some(event);
         }

         // 如果有未读，标记到最后一条
         if let Some(last) = last_unread {
             mark_read(state, group_id, actor_id, &last.id)?;
         }

         Ok(())
     }
     ```

     **ack_message 函数**:
     ```rust
     /// ACK 重要消息（写入 ChatAck 事件到账本）
     ///
     /// 用于确认收到 priority="attention" 的消息
     /// 幂等操作：如果已经 ACK 过则静默成功
     ///
     /// 参考: cccc/src/cccc/kernel/inbox.py:152-169 has_chat_ack
     pub fn ack_message(
         state: &AppState,
         group_id: &str,
         actor_id: &str,
         event_id: &str,
     ) -> Result<()> {
         let group_dir = state.groups_dir.join(group_id);
         let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
         let lock_path = group_dir.join("state/ledger/ledger.lock");

         crate::group::load_group(&group_dir)
             .map_err(|_| MessagingError::GroupNotFound(group_id.to_string()))?;

         // 验证目标事件存在
         let _target = ghostcode_ledger::iter_events(&ledger_path)?
             .filter_map(|r| r.ok())
             .find(|e| e.id == event_id)
             .ok_or_else(|| MessagingError::EventNotFound(event_id.to_string()))?;

         // 幂等检查：是否已经 ACK 过
         let already_acked = ghostcode_ledger::iter_events(&ledger_path)?
             .filter_map(|r| r.ok())
             .any(|e| {
                 e.kind == EventKind::ChatAck
                     && e.data.get("event_id").and_then(|v| v.as_str()) == Some(event_id)
                     && e.data.get("actor_id").and_then(|v| v.as_str()) == Some(actor_id)
             });

         if already_acked {
             return Ok(());
         }

         // 写入 ChatAck 事件
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
     ```

- **验收标准**:
  - `cargo check -p ghostcode-daemon` 零错误
  - `unread_messages`, `mark_read`, `mark_all_read`, `unread_count`, `ack_message` 已导出
  - MessagingError 已移至 mod.rs，send.rs 通过 `use super::` 引用

### Task 2: 连接 dispatch handler
- **类型**: 后端
- **文件范围**:
  - `crates/ghostcode-daemon/src/dispatch.rs` (修改)
- **依赖**: Task 1
- **实施步骤**:

  1. 在 dispatch.rs 顶部添加导入:
     ```rust
     use crate::messaging::inbox;
     ```

  2. 将 `"inbox_list"` stub 替换为:
     ```rust
     "inbox_list" => handle_inbox_list(state, &req.args).await,
     ```

  3. 将 `"inbox_mark_read"` stub 替换为:
     ```rust
     "inbox_mark_read" => handle_inbox_mark_read(state, &req.args).await,
     ```

  4. 将 `"inbox_mark_all_read"` stub 替换为:
     ```rust
     "inbox_mark_all_read" => handle_inbox_mark_all_read(state, &req.args).await,
     ```

  5. 实现 `handle_inbox_list` handler:
     ```rust
     /// inbox_list handler
     ///
     /// 获取 Actor 的未读消息列表
     ///
     /// 必填参数：group_id, actor_id
     /// 可选参数：limit (默认 50)
     async fn handle_inbox_list(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
         let group_id = match args["group_id"].as_str() {
             Some(v) => v,
             None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
         };
         let actor_id = match args["actor_id"].as_str() {
             Some(v) => v,
             None => return DaemonResponse::err("INVALID_ARGS", "missing required field: actor_id"),
         };
         let limit = args["limit"].as_u64().unwrap_or(50) as usize;

         match inbox::unread_messages(state, group_id, actor_id, limit) {
             Ok(messages) => DaemonResponse::ok(serde_json::json!({
                 "messages": serde_json::to_value(&messages).unwrap_or_default(),
                 "count": messages.len(),
             })),
             Err(e) => DaemonResponse::err("MESSAGING_ERROR", e.to_string()),
         }
     }
     ```

  6. 实现 `handle_inbox_mark_read` handler:
     ```rust
     /// inbox_mark_read handler
     ///
     /// 标记已读到指定事件
     ///
     /// 必填参数：group_id, actor_id, event_id
     async fn handle_inbox_mark_read(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
         let group_id = match args["group_id"].as_str() {
             Some(v) => v,
             None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
         };
         let actor_id = match args["actor_id"].as_str() {
             Some(v) => v,
             None => return DaemonResponse::err("INVALID_ARGS", "missing required field: actor_id"),
         };
         let event_id = match args["event_id"].as_str() {
             Some(v) => v,
             None => return DaemonResponse::err("INVALID_ARGS", "missing required field: event_id"),
         };

         match inbox::mark_read(state, group_id, actor_id, event_id) {
             Ok(()) => DaemonResponse::ok(serde_json::json!({ "marked": true })),
             Err(e) => DaemonResponse::err("MESSAGING_ERROR", e.to_string()),
         }
     }
     ```

  7. 实现 `handle_inbox_mark_all_read` handler:
     ```rust
     /// inbox_mark_all_read handler
     ///
     /// 全部标记已读
     ///
     /// 必填参数：group_id, actor_id
     async fn handle_inbox_mark_all_read(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
         let group_id = match args["group_id"].as_str() {
             Some(v) => v,
             None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
         };
         let actor_id = match args["actor_id"].as_str() {
             Some(v) => v,
             None => return DaemonResponse::err("INVALID_ARGS", "missing required field: actor_id"),
         };

         match inbox::mark_all_read(state, group_id, actor_id) {
             Ok(()) => DaemonResponse::ok(serde_json::json!({ "marked_all": true })),
             Err(e) => DaemonResponse::err("MESSAGING_ERROR", e.to_string()),
         }
     }
     ```

  注意：handler 函数虽然标记为 async（保持与现有 handler 一致），但 inbox 函数本身是同步的。这没问题，async fn 可以调用同步函数。

- **验收标准**:
  - `cargo check -p ghostcode-daemon` 零错误
  - dispatch("inbox_list", ...) 不再返回 NOT_IMPLEMENTED
  - dispatch("inbox_mark_read", ...) 不再返回 NOT_IMPLEMENTED
  - dispatch("inbox_mark_all_read", ...) 不再返回 NOT_IMPLEMENTED

### Task 3: TDD 测试套件
- **类型**: 后端测试
- **文件范围**:
  - `crates/ghostcode-daemon/tests/inbox_test.rs` (新建)
- **依赖**: Task 1
- **实施步骤**:

  1. 创建 `crates/ghostcode-daemon/tests/inbox_test.rs`

  2. 文件头注释（标准格式）

  3. 导入（参考 send_test.rs 模式）:
     ```rust
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
     ```

  4. setup 辅助函数（完全复用 send_test.rs 的模式）:
     ```rust
     fn make_actor(actor_id: &str, role: ActorRole, runtime: RuntimeKind) -> ActorInfo { ... }
     async fn setup() -> (TempDir, Arc<AppState>, GroupInfo) { ... }
     ```

  5. 实现 7 个测试用例:

     **#[tokio::test] async fn unread_messages_returns_new()**
     - setup → claude 发送 3 条消息给 codex
     - 调用 unread_messages(state, group_id, "codex", 50)
     - assert 返回 3 条
     - assert 每条 data["text"] 和发送时一致

     **#[tokio::test] async fn mark_read_advances_cursor()**
     - setup → claude 发送 3 条消息给 codex
     - 获取 unread，取第 2 条的 event_id
     - mark_read(state, group_id, "codex", &second_event_id)
     - 再次 unread_messages → assert 返回 1 条（第 3 条）

     **#[tokio::test] async fn mark_all_read_clears_inbox()**
     - setup → claude 发送 5 条消息给 codex
     - mark_all_read(state, group_id, "codex")
     - unread_messages → assert 返回 0 条
     - unread_count → assert == 0

     **#[tokio::test] async fn unread_count_consistent()**
     - setup → claude 发送 3 条给 codex，2 条给 gemini
     - unread_count(codex) == unread_messages(codex).len() == 3
     - unread_count(gemini) == unread_messages(gemini).len() == 2

     **#[tokio::test] async fn cursor_monotonic()**
     - setup → claude 发送 3 条消息给 codex
     - 获取 3 条的 event_id
     - mark_read 第 3 条 → mark_read 第 1 条（尝试回退）
     - unread_messages → assert 返回 0 条（游标保持在第 3 条，不回退）

     **#[tokio::test] async fn ack_message_creates_event()**
     - setup → claude 发送 1 条消息给 codex
     - ack_message(state, group_id, "codex", &event_id)
     - iter_events 过滤 kind == ChatAck
     - assert 找到 1 条，data["event_id"] == event_id

     **#[tokio::test] async fn mark_read_creates_chat_read_event()**
     - setup → claude 发送 1 条给 codex
     - mark_read(state, group_id, "codex", &event_id)
     - iter_events 过滤 kind == ChatRead
     - assert 找到 1 条，data["event_id"] == event_id, data["actor_id"] == "codex"

- **验收标准**:
  - `cargo test -p ghostcode-daemon --test inbox_test` 全部通过
  - 7 个测试用例覆盖核心场景 + 边界情况（游标单调性）

## 文件冲突检查
- Task 1 修改: messaging/mod.rs(修改), messaging/send.rs(修改错误类型引用), messaging/inbox.rs(新建)
- Task 2 修改: dispatch.rs
- Task 3 修改: tests/inbox_test.rs(新建)
- **潜在冲突**: Task 1 修改 send.rs（仅删除错误类型定义，改为 use super::），Task 2 和 Task 3 不涉及 send.rs
- **结论**: 无文件冲突

## 并行分组
- **Layer 1** (独立): Task 1 — 核心模块实现 + MessagingError 重构
- **Layer 2** (并行，依赖 Layer 1): Task 2 + Task 3 — dispatch 接入 + 测试

## Builder 配置
- Builder 数量: 3 个（Sonnet）
- Layer 1: 1 个 Builder
- Layer 2: 2 个 Builder 并行
