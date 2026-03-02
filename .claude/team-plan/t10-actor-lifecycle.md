# Team Plan: T10 Actor 生命周期管理

## 概述
实现 Actor 启停 + Headless 状态管理 + 心跳超时检测，使 Agent 能在 Group 内启动/停止，并通过 Headless 协议报告工作状态。

## Codex 分析摘要
Codex 异常退出（状态码 1），未获得有效分析。以下由 Claude 基于代码库和 CCCC 源码独立完成。

## Gemini 分析摘要
Gemini 配额耗尽（TerminalQuotaError），未获得有效分析。以下由 Claude 基于代码库和 CCCC 源码独立完成。

## Claude 独立分析

### 现有代码基础
- `server.rs`: AppState 仅含 `shutdown: Notify`，需扩展
- `dispatch.rs`: `actor_start/actor_stop/headless_status/headless_set_status` 四个 op 为 stub
- `actor_mgmt.rs`: Actor 注册/移除已实现，操作 GroupInfo.actors
- `group.rs`: Group CRUD 已实现，操作 group.yaml + ledger
- `types/actor.rs`: ActorInfo 含 `running: bool` 和 `pid: Option<u32>` 字段

### CCCC 参考架构（cccc/src/cccc/runners/headless.py）
- HeadlessSession: status, current_task_id, last_message_id, started_at, updated_at
- HeadlessSupervisor: 全局管理器，线程安全，存储 `Dict[(group_id, actor_id), HeadlessSession]`
- 被动心跳：基于 updated_at 时戳判断超时
- 状态持久化到文件（GhostCode 简化：仅靠 group.yaml 中 actor.running 标志）
- 恢复流程：依赖 group.running + actor.enabled 标志

### 关键技术决策
1. **HeadlessSession 使用 `tokio::sync::RwLock`**（非 std::sync），因为需要在异步上下文中持锁
2. **心跳检测使用 tokio::spawn 后台 task**，每 10s 检查一次 updated_at
3. **AppState 扩展**：添加 `sessions`、`groups_dir`、`event_tx` 字段
4. **Actor running 标志更新**：start/stop 时同步更新 group.yaml 中 actor.running
5. **Phase 1 简化**：不持久化独立的 runner state 文件，仅靠 group.yaml actor.running 恢复

## 子任务列表

### Task 1: 创建 runner.rs - Headless 数据结构
- **类型**: 后端
- **文件范围**: `crates/ghostcode-daemon/src/runner.rs`（新建）
- **依赖**: 无
- **实施步骤**:
  1. 创建文件头注释（@author Atlas.oi @date 2026-03-01）
  2. 定义 `HeadlessStatus` 枚举：`Idle, Working, Waiting, Stopped`（derive Serialize/Deserialize/Debug/Clone/PartialEq）
  3. 定义 `HeadlessSession` 结构体：
     ```rust
     pub struct HeadlessSession {
         pub group_id: String,
         pub actor_id: String,
         pub status: HeadlessStatus,
         pub current_task_id: Option<String>,
         pub last_message_id: Option<String>,
         pub started_at: String,      // ISO 8601
         pub updated_at: String,      // ISO 8601，心跳时戳
     }
     ```
  4. 为 HeadlessSession 实现方法：
     - `new(group_id, actor_id) -> Self`：初始化状态为 Idle，started_at/updated_at 为当前时间
     - `set_status(&mut self, status: HeadlessStatus, task_id: Option<String>)`：更新 status + current_task_id + updated_at
     - `set_last_message(&mut self, message_id: String)`：更新 last_message_id + updated_at
     - `is_timed_out(&self, timeout_secs: u64) -> bool`：判断 updated_at 距今是否超过 timeout_secs
     - `to_state(&self) -> HeadlessState`：转换为可序列化的状态快照
  5. 定义 `HeadlessState` 结构体（Serialize/Deserialize，用于 API 响应）：
     ```rust
     #[derive(Debug, Clone, Serialize, Deserialize)]
     pub struct HeadlessState {
         pub v: u8,
         pub group_id: String,
         pub actor_id: String,
         pub status: HeadlessStatus,
         pub current_task_id: Option<String>,
         pub last_message_id: Option<String>,
         pub started_at: String,
         pub updated_at: String,
     }
     ```
  6. 定义 `LifecycleError` 错误类型：
     ```rust
     #[derive(Debug, thiserror::Error)]
     pub enum LifecycleError {
         Io(std::io::Error),
         Yaml(serde_yaml::Error),
         Ledger(ghostcode_ledger::LedgerError),
         ActorNotFound(String),
         GroupNotFound(String),
         SessionNotFound { group_id: String, actor_id: String },
         SessionAlreadyExists { group_id: String, actor_id: String },
         ActorError(crate::actor_mgmt::ActorError),
         GroupError(crate::group::GroupError),
     }
     ```
- **验收标准**: `cargo check -p ghostcode-daemon` 编译通过（runner.rs 需要在 lib.rs 注册才能检查，但 Task 1 只负责文件创建，注册在 Task 3）

### Task 2: 扩展 AppState - 添加运行时状态管理
- **类型**: 后端
- **文件范围**: `crates/ghostcode-daemon/src/server.rs`（修改）
- **依赖**: 无（Task 2 先定义 AppState 结构，runner.rs 的类型引用在 Task 3 集成时再添加）
- **实施步骤**:
  1. 在 server.rs 顶部添加必要 import：
     ```rust
     use std::collections::HashMap;
     use std::path::PathBuf;
     use tokio::sync::{RwLock, broadcast};
     ```
  2. 扩展 AppState 结构体（注意：此时不引用 runner.rs 类型，用泛型 Value 占位或直接定义 session 类型）：
     ```rust
     pub struct AppState {
         /// 关闭信号
         shutdown: Notify,
         /// Groups 目录路径
         pub groups_dir: PathBuf,
         /// Headless session 存储：key = (group_id, actor_id)
         pub sessions: Arc<RwLock<HashMap<(String, String), serde_json::Value>>>,
         /// 事件广播通道发送端
         pub event_tx: broadcast::Sender<ghostcode_types::event::Event>,
     }
     ```
     **重要修改**：此处先用 `serde_json::Value` 作为 session 类型占位，Task 3 集成时替换为 `HeadlessSession`
  3. 更新 `AppState::new()` 方法签名，接受 `groups_dir: PathBuf` 参数：
     ```rust
     pub fn new(groups_dir: PathBuf) -> Self {
         let (event_tx, _) = broadcast::channel(1024);
         Self {
             shutdown: Notify::new(),
             groups_dir,
             sessions: Arc::new(RwLock::new(HashMap::new())),
             event_tx,
         }
     }
     ```
  4. 保留 `trigger_shutdown()` 方法不变
  5. 更新 Default 实现（使用默认 groups_dir）：
     ```rust
     impl Default for AppState {
         fn default() -> Self {
             Self::new(PathBuf::from("/tmp/ghostcode-test/groups"))
         }
     }
     ```
  6. 更新现有测试中的 `AppState::new()` 调用（如果有的话），传入 groups_dir 参数
- **验收标准**: `cargo check -p ghostcode-daemon` 编译通过，现有测试通过

### Task 3: 创建 lifecycle.rs + 模块集成
- **类型**: 后端
- **文件范围**: `crates/ghostcode-daemon/src/lifecycle.rs`（新建）+ `crates/ghostcode-daemon/src/lib.rs`（修改）
- **依赖**: Task 1, Task 2
- **实施步骤**:
  1. 在 lib.rs 中注册新模块：`pub mod runner;` 和 `pub mod lifecycle;`
  2. 创建 lifecycle.rs 文件头注释
  3. 将 server.rs 中 AppState.sessions 的类型从 `serde_json::Value` 替换为 `crate::runner::HeadlessSession`
  4. 实现 `start_actor`：
     ```rust
     pub async fn start_actor(
         state: &AppState,
         group_id: &str,
         actor_id: &str,
     ) -> Result<(), LifecycleError>
     ```
     逻辑：
     - 加载 group.yaml 验证 actor 存在
     - 检查 session 不重复存在
     - 创建 HeadlessSession（status=Idle）
     - 插入 state.sessions
     - 更新 actor.running = true 到 group.yaml
     - 写入 ActorStart 事件到账本
  5. 实现 `stop_actor`（幂等）：
     ```rust
     pub async fn stop_actor(
         state: &AppState,
         group_id: &str,
         actor_id: &str,
     ) -> Result<(), LifecycleError>
     ```
     逻辑：
     - 从 state.sessions 移除 session（不存在也不报错 = 幂等）
     - 更新 actor.running = false 到 group.yaml
     - 写入 ActorStop 事件到账本
     - 通过 event_tx 广播事件（如果 Foreman 在线则收到 SystemNotify）
  6. 实现 `get_headless_status`：
     ```rust
     pub async fn get_headless_status(
         state: &AppState,
         group_id: &str,
         actor_id: &str,
     ) -> Result<Option<HeadlessState>, LifecycleError>
     ```
  7. 实现 `set_headless_status`：
     ```rust
     pub async fn set_headless_status(
         state: &AppState,
         group_id: &str,
         actor_id: &str,
         status: HeadlessStatus,
         task_id: Option<String>,
     ) -> Result<HeadlessState, LifecycleError>
     ```
  8. 实现 `restore_running_actors`：
     ```rust
     pub async fn restore_running_actors(state: &AppState) -> Result<(), LifecycleError>
     ```
     逻辑：
     - 扫描 groups_dir 下所有 group.yaml
     - 对每个 group 中 running == true 的 actor
     - 创建 HeadlessSession 并插入 state.sessions
  9. 实现 `spawn_heartbeat_monitor`：
     ```rust
     pub fn spawn_heartbeat_monitor(
         state: Arc<AppState>,
         timeout_secs: u64,  // 默认 60
     ) -> tokio::task::JoinHandle<()>
     ```
     逻辑：
     - tokio::spawn 后台 task
     - 每 10 秒检查所有 session 的 updated_at
     - 超时的 session：
       a. 写入 SystemNotify 事件到账本（通知 Foreman）
       b. 通过 event_tx 广播
       c. 标记 session status = Stopped
- **验收标准**: `cargo check -p ghostcode-daemon` 编译通过

### Task 4: 更新 dispatch.rs - 实现 4 个 handler
- **类型**: 后端
- **文件范围**: `crates/ghostcode-daemon/src/dispatch.rs`（修改）
- **依赖**: Task 3
- **实施步骤**:
  1. 添加 import：`use crate::{lifecycle, runner::HeadlessStatus};`
  2. 将 dispatch 函数改为 async（已经是 async）
  3. 替换 `actor_start` 的 stub：
     ```rust
     "actor_start" => handle_actor_start(state, req.args).await,
     ```
     实现 `handle_actor_start`：从 args 提取 group_id + actor_id → 调用 lifecycle::start_actor → 返回成功/失败
  4. 替换 `actor_stop` 的 stub：
     类似，调用 lifecycle::stop_actor
  5. 替换 `headless_status` 的 stub：
     从 args 提取 group_id + actor_id → 调用 lifecycle::get_headless_status → 返回 HeadlessState JSON
  6. 替换 `headless_set_status` 的 stub：
     从 args 提取 group_id + actor_id + status + task_id → 调用 lifecycle::set_headless_status → 返回更新后的 HeadlessState
  7. 更新 handle_ping：从 state.sessions 计算 has_unread（Phase 1 简化：始终 false，T13 再实现真实逻辑）
- **验收标准**: `cargo check -p ghostcode-daemon` 编译通过，dispatch_test 仍然通过

### Task 5: 创建 lifecycle_test.rs 测试套件
- **类型**: 后端
- **文件范围**: `crates/ghostcode-daemon/tests/lifecycle_test.rs`（新建）
- **依赖**: Task 3, Task 4
- **实施步骤**:
  1. 创建文件头注释
  2. 编写辅助函数 `setup() -> (TempDir, Arc<AppState>, GroupInfo)`：
     - 创建临时目录
     - 创建 AppState
     - 创建 Group + 添加 3 个 Actor（claude/Foreman, codex/Peer, gemini/Peer）
  3. 编写测试用例：
     - `#[tokio::test] async fn start_actor_sets_idle()`：start → get_status == Idle
     - `#[tokio::test] async fn stop_actor_cleans_session()`：start → stop → get_status 返回 None
     - `#[tokio::test] async fn stop_idempotent()`：start → stop → stop → 不报错
     - `#[tokio::test] async fn status_transitions()`：start(Idle) → set(Working) → set(Waiting) → set(Idle) → 全部成功
     - `#[tokio::test] async fn restore_running_actors()`：创建 Group + 3 Actor(running=true) → 新 AppState → restore → sessions 中有 3 个
     - `#[tokio::test] async fn heartbeat_timeout_detection()`：start → 手动设置 updated_at 为 70s 前 → 运行心跳检查 → session 状态变为 Stopped
     - `#[tokio::test] async fn start_nonexistent_actor_fails()`：尝试 start 不存在的 actor → 返回错误
     - `#[tokio::test] async fn start_duplicate_session_fails()`：start 同一 actor 两次 → 第二次返回错误
  4. 运行 `cargo test -p ghostcode-daemon --test lifecycle_test` 确保全部通过
  5. 运行 `cargo clippy -p ghostcode-daemon -- -D warnings` 确保零警告
- **验收标准**: 所有 8 个测试通过 + clippy 零警告

## 文件冲突检查

| Task | 文件 | 操作 |
|------|------|------|
| Task 1 | `crates/ghostcode-daemon/src/runner.rs` | 新建 |
| Task 2 | `crates/ghostcode-daemon/src/server.rs` | 修改 |
| Task 3 | `crates/ghostcode-daemon/src/lifecycle.rs` | 新建 |
| Task 3 | `crates/ghostcode-daemon/src/lib.rs` | 修改 |
| Task 3 | `crates/ghostcode-daemon/src/server.rs` | 修改（替换 Value→HeadlessSession） |
| Task 4 | `crates/ghostcode-daemon/src/dispatch.rs` | 修改 |
| Task 5 | `crates/ghostcode-daemon/tests/lifecycle_test.rs` | 新建 |

- Task 2 和 Task 3 都修改 server.rs → 通过依赖关系解决（Task 3 依赖 Task 2）
- 其余文件范围无冲突

## 并行分组

- **Layer 1** (并行): Task 1 (runner.rs), Task 2 (server.rs)
- **Layer 2** (依赖 Layer 1): Task 3 (lifecycle.rs + lib.rs + server.rs 类型替换)
- **Layer 3** (依赖 Layer 2): Task 4 (dispatch.rs), Task 5 (tests) — 可并行但 Task 5 需要 Task 4 的 dispatch 集成才能做完整测试
- **推荐执行**: Layer 1 (2 并行) → Layer 2 (1 串行) → Layer 3 (2 并行)

## 风险评估

1. **AppState 扩展可能破坏现有测试**: server_test.rs 和 dispatch_test.rs 中的 AppState::new() 调用需要更新。Task 2 负责处理。
2. **心跳检测的时间模拟**: lifecycle_test.rs 中的超时测试需要手动修改 updated_at 而非真正等待 60s。
3. **RwLock 死锁风险**: lifecycle.rs 中持锁调用其他持锁方法可能死锁。解决：每个方法单独获取锁，不嵌套。
