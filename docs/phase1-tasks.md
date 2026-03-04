# GhostCode Phase 1 开发任务清单

> 目标：三个 Agent（Claude Code + Codex CLI + Gemini CLI）通过 GhostCode Daemon 协作通信
> 日期：2026-02-28
> 状态：规划完成，待实施

---

## MVP 成功判据

| # | 判据 | 验证方式 |
|---|------|---------|
| OK-1 | Claude Code + Codex CLI + Gemini CLI 注册为 Actor，互发消息并收到已读回执 | 集成测试：3 个 Actor 互发消息 |
| OK-2 | 消息持久化到事件账本，Daemon 重启后历史完整恢复 | 测试：写入 N 条消息 → 重启 → 读取验证 N 条完整 |
| OK-3 | Agent 异常退出时 Daemon 在 5s 内检测并通知 Foreman | 测试：kill Actor 进程 → 验证 Foreman 收到通知 |
| OK-4 | TS Plugin 通过 IPC 调用 Daemon，延迟 < 100ms | 基准测试：1000 次 ping 的 p99 延迟 |
| OK-5 | macOS arm64 单二进制直接运行 | CI：`cargo build --release --target aarch64-apple-darwin` |
| OK-6 | Plugin 安装后自动下载 Rust 二进制 | 端到端：`claude plugin install` → 验证 daemon 可启动 |

---

## 技术选型（已锁定）

| 类别 | 选择 | 用途 |
|------|------|------|
| 异步运行时 | `tokio` (features: full) | 异步 I/O、定时器、信号处理 |
| 序列化 | `serde` + `serde_json` | JSON 序列/反序列化 |
| 文件锁 | `fs2` | 单实例锁 + 账本写锁 |
| PTY | `nix` (feature: pty) | Unix PTY 操作（macOS/Linux） |
| UUID | `uuid` (v4) | 事件 ID 生成 |
| 时间 | `chrono` (utc) | ISO 8601 时间戳 |
| CLI | `clap` (derive) | 命令行参数解析 |
| 日志 | `tracing` + `tracing-subscriber` | 结构化日志 |
| 错误处理 | `thiserror` + `anyhow` | 错误类型定义 |
| TS 构建 | `pnpm` + `tsup` | Plugin 打包 |
| PBT 框架 | `proptest` | 属性基测试（自动生成边界测试数据） |
| 契约测试 | JSON Schema | Rust ↔ TS 接口契约验证 |
| 变异测试 | `cargo-mutants`（CI 阶段） | 测试质量审计 |

---

## 测试驱动开发策略（TDD + PBT 混合）

### 核心理念

GhostCode 采用 **TDD + PBT 混合驱动**，而非纯 TDD：

```
传统 TDD：先写具体案例测试 → 只验证你想到的场景
TDD + PBT：先写属性不变量 → 框架自动生成 1000+ 边界数据验证

GhostCode 策略：
  🔴 Red    → 从 PBT 属性写失败的属性测试 + 关键路径的 TDD 案例测试
  🟢 Green  → 写最少代码让所有测试通过
  🔵 Refactor → 重构，测试仍通过
```

### 四层测试金字塔

```
        ╱  Mutation Testing  ╲        ← CI 定期（质量审计）
       ╱  BDD/集成测试 (场景)  ╲       ← T19 端到端
      ╱  Contract Testing (契约)╲      ← Rust↔TS 接口
     ╱  TDD + PBT (单元 + 属性)  ╲     ← 每个任务的核心
```

| 层级 | 方法 | 时机 | 工具 |
|------|------|------|------|
| **L1 单元+属性** | TDD + PBT | 每个任务开发时 | `cargo test` + `proptest` |
| **L2 契约** | JSON Schema 契约 | T06/T14/T18 开发时 | 共享 schema 文件 |
| **L3 集成** | BDD 场景 | T19 阶段 | `cargo test --test integration` |
| **L4 变异** | Mutation Testing | CI/CD | `cargo mutants` |

### 每个任务的 TDD 工作流模板

```
任务 T0x 开发流程：

1. 创建测试文件
   → crates/<crate>/tests/<module>_test.rs（集成测试）
   → crates/<crate>/src/<module>.rs 底部 #[cfg(test)] mod tests（单元测试）

2. 🔴 Red：编写失败测试
   a. 从任务的 PBT 属性 → proptest! 宏
   b. 从任务的约束 → #[test] 案例测试
   c. cargo test → 全部失败（编译错误或断言失败）

3. 🟢 Green：最小实现
   a. 实现公共 API（仅任务清单定义的签名）
   b. cargo test → 全部通过
   c. 不做任何额外优化

4. 🔵 Refactor：重构
   a. 提取重复代码
   b. 改善命名和结构
   c. cargo test → 仍然全部通过
   d. cargo clippy → 零警告
```

### Rust PBT 示例模板

```rust
use proptest::prelude::*;

proptest! {
    /// PBT 属性：往返性
    #[test]
    fn roundtrip_event_serialization(
        kind in prop_oneof![
            Just(EventKind::GroupCreate),
            Just(EventKind::ChatMessage),
            // ... 所有 EventKind
        ],
        body in "\\PC{0,100}",  // 任意 Unicode 字符串
    ) {
        let event = Event::new(kind, body);
        let json = serde_json::to_string(&event).unwrap();
        let restored: Event = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(event, restored);
    }

    /// PBT 属性：单调性
    #[test]
    fn ledger_append_monotonic(
        events in prop::collection::vec(arb_event(), 1..100),
    ) {
        let dir = tempdir().unwrap();
        let ledger = dir.path().join("ledger.jsonl");
        let lock = dir.path().join("ledger.lock");
        for e in &events {
            append_event(&ledger, &lock, e).unwrap();
        }
        prop_assert_eq!(count_events(&ledger).unwrap(), events.len());
    }
}
```

### 产品化路线：双阶段 TDD+PBT（Phase 2-3）

GhostCode 作为开发平台时，TDD+PBT 贯穿**开发 + 验证**两个阶段：

#### 阶段一：开发时（Agent 写代码时）— Phase 2 集成

Agent 在执行编码任务时，自动遵循 TDD+PBT 循环：

```
用户需求 "实现用户登录"
  ↓
Agent 分析需求 → 提取 PBT 属性不变量
  例：登录成功后 session 必须存在
  例：密码错误时 session 不得创建
  例：并发登录不产生重复 session
  ↓
🔴 Red:  Agent 先写 proptest 属性测试 + TDD 案例测试
🟢 Green: Agent 写最小实现让测试通过
🔵 Refactor: Agent 重构代码，测试仍通过
  ↓
测试全通过 → 提交给 Ralph 验证
```

实现方式：在多模型路由的 Prompt 模板中注入 TDD 指令
- 参考：ccg-workflow `templates/commands/execute.md` 的 5 步流程
- 增加第 0 步：从需求提取 PBT 属性 → 生成测试文件

#### 阶段二：验证时（Ralph 审核时）— Phase 3 集成

Ralph 验证引擎增强为 10 项检查，审计 Agent 写的测试质量：

```
当前 Ralph 7 项检查：
  BUILD → TEST → LINT → FUNCTIONALITY → ARCHITECT → TODO → ERROR_FREE

Phase 3 增强为 10 项检查：
  BUILD → TEST → LINT → PBT_VERIFY → MUTATION_SCORE → CONTRACT_CHECK
  → FUNCTIONALITY → ARCHITECT → TODO → ERROR_FREE

新增 3 项（质量门禁）：
  PBT_VERIFY     - 检查 Agent 是否写了 PBT 属性测试（覆盖率 > 60%）
  MUTATION_SCORE  - 运行 cargo-mutants → 变异存活率 < 20% 才通过
  CONTRACT_CHECK  - 验证跨语言接口契约一致
```

#### 两阶段的职责分工

```
开发阶段（Agent）         验证阶段（Ralph）
  写测试 + 写代码            跑测试 + 审质量
  ↓                          ↓
  TDD 驱动开发节奏           确认测试存在且通过
  PBT 提取属性不变量         检查 PBT 覆盖率达标
  不做 Mutation（太慢）      审计 Mutation 存活率
  ↓                          ↓
  快速迭代                   严格把关
```

核心原则：**开发时严格把控 > 测试时事后补救**。
Agent 在开发阶段就遵循 TDD+PBT，Ralph 在验证阶段做最终审计，两者缺一不可。

---

## 构建顺序总览

```
阶段 A: 项目脚手架
  └─ T01 Cargo workspace 初始化
  └─ T02 核心类型定义

阶段 B: 事件账本（系统真理来源）
  └─ T03 NDJSON 账本读写
  └─ T04 Blob 溢出处理

阶段 C: Daemon 骨架
  └─ T05 单实例锁 + 进程管理
  └─ T06 Unix Socket 监听 + JSON-RPC 协议层
  └─ T07 请求分发框架 (dispatch)

阶段 D: Agent 管理
  └─ T08 Group 数据模型 + 持久化
  └─ T09 Actor 注册/发现/移除
  └─ T10 Actor 生命周期（启停 + 异常检测）

阶段 E: 消息投递
  └─ T11 消息发送 + 事件写入
  └─ T12 Inbox 读取 + 已读游标
  └─ T13 投递引擎 (DeliveryThrottle)

阶段 F: MCP 工具层
  └─ T14 stdio JSON-RPC 2.0 Server 框架
  └─ T15 核心 MCP 工具实现（8 个）

阶段 G: TS Plugin 薄壳
  └─ T16 Plugin 项目初始化
  └─ T17 Daemon 生命周期管理（启动/停止/心跳）
  └─ T18 IPC 桥接层

阶段 H: 集成 + 分发
  └─ T19 三 Agent 端到端集成测试
  └─ T20 跨平台构建 + Plugin 打包
```

---

## 任务详细定义

### 阶段 A：项目脚手架

#### T01 - Cargo Workspace 初始化

**前置依赖**: 无
**产出**: `Cargo.toml` (workspace) + `crates/` 目录结构

```
GhostCode/
  Cargo.toml              ← workspace 根
  crates/
    ghostcode-types/      ← 核心类型（Event, Request, Response）
      Cargo.toml
      src/lib.rs
    ghostcode-ledger/     ← 事件账本
      Cargo.toml
      src/lib.rs
    ghostcode-daemon/     ← Daemon 主进程
      Cargo.toml
      src/main.rs
    ghostcode-mcp/        ← MCP Server
      Cargo.toml
      src/lib.rs
  src/                    ← 保留：未来放置集成测试
    plugin/               ← TS Plugin 薄壳
```

**约束**:
- workspace 根 `Cargo.toml` 统一管理依赖版本 (`[workspace.dependencies]`)
- 每个 crate 独立可编译 (`cargo build -p ghostcode-types`)
- edition = "2021", rust-version = "1.75"

**PBT 属性**:
- `cargo check --workspace` 零错误零警告

**TDD 步骤**:
```
🔴 Red:   无（脚手架任务无测试）
🟢 Green: 创建 workspace + 所有 crate 目录 + Cargo.toml
🔵 Check: cargo check --workspace 零错误
```

---

#### T02 - 核心类型定义

**前置依赖**: T01
**产出**: `ghostcode-types/src/` 下的所有类型定义

**需要定义的类型**:

```rust
// === 事件相关 ===

/// 事件类型枚举（Phase 1 子集：14 种）
enum EventKind {
    // Group 生命周期
    GroupCreate, GroupUpdate, GroupStart, GroupStop, GroupSetState,
    // Actor 生命周期
    ActorAdd, ActorUpdate, ActorStart, ActorStop, ActorRemove,
    // 消息
    ChatMessage, ChatRead, ChatAck,
    // 系统
    SystemNotify,
}

/// 事件结构体
struct Event {
    v: u8,                    // 固定 1
    id: String,               // uuid v4 hex (32字符)
    ts: String,               // ISO 8601 UTC 微秒精度
    kind: EventKind,
    group_id: String,
    scope_key: String,
    by: String,               // actor_id 或 "user"
    data: serde_json::Value,
}

// === IPC 协议 ===

/// Daemon 请求
struct DaemonRequest {
    v: u8,                    // 固定 1
    op: String,
    args: serde_json::Value,
}

/// Daemon 响应
struct DaemonResponse {
    v: u8,
    ok: bool,
    result: serde_json::Value,
    error: Option<DaemonError>,
}

struct DaemonError {
    code: String,
    message: String,
}

// === Actor 相关 ===

/// Actor 角色
enum ActorRole { Foreman, Peer }

/// Actor 运行时类型
enum RuntimeKind { Claude, Codex, Gemini, Custom(String) }

/// Actor 状态
struct ActorInfo {
    actor_id: String,
    display_name: String,
    role: ActorRole,
    runtime: RuntimeKind,
    running: bool,
    // ...
}

// === Group 相关 ===

/// Group 状态
enum GroupState { Active, Idle, Paused, Stopped }

struct GroupInfo {
    group_id: String,
    title: String,
    state: GroupState,
    actors: Vec<ActorInfo>,
    // ...
}

// === 端点描述符 ===
// 参考: cccc/src/cccc/daemon/server.py:375-434
struct AddrDescriptor {
    v: u8,
    transport: String,       // "unix"
    path: String,            // socket 路径
    pid: u32,
    version: String,
    ts: String,
}
```

**约束**:
- 所有类型 derive `Serialize, Deserialize, Debug, Clone`
- EventKind 序列化为 `"group.create"` 格式（snake_case + 点分隔）
- Event.id 使用 `uuid::Uuid::new_v4().simple().to_string()`
- Event.ts 使用 `chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)`

**PBT 属性**:
- 往返性：`Event → JSON → Event` 完全相等
- EventKind 序列化/反序列化对称：每种 kind 都能往返

**TDD 步骤**:
```
🔴 Red: 先写测试文件 crates/ghostcode-types/src/lib.rs #[cfg(test)]
   - #[test] fn event_kind_serialize_format()
     → 验证 EventKind::GroupCreate 序列化为 "group.create"
   - proptest! fn roundtrip_event(event in arb_event())
     → Event → JSON → Event 相等
   - proptest! fn roundtrip_all_event_kinds(kind in arb_event_kind())
     → 每种 kind 都能序列化/反序列化
   - #[test] fn event_id_format()
     → 验证 id 是 32 字符十六进制
   - #[test] fn event_ts_format()
     → 验证 ts 是 ISO 8601 UTC 微秒精度
   → cargo test 编译失败（类型不存在）

🟢 Green: 实现所有类型定义
   → cargo test 全部通过

🔵 Refactor: 提取 arb_event() 策略到 testutil 模块供后续任务复用
```

---

### 阶段 B：事件账本

#### T03 - NDJSON 账本读写

**前置依赖**: T02
**产出**: `ghostcode-ledger/src/` 账本核心功能
**参考**: `cccc/src/cccc/kernel/ledger.py:60-131`

**公共 API**:

```rust
/// 追加事件到账本
/// 双重保护：flock 写锁 + 原子追加
fn append_event(ledger_path: &Path, lock_path: &Path, event: &Event) -> Result<()>

/// 读取最后 N 行（二进制反向扫描）
fn read_last_lines(ledger_path: &Path, n: usize) -> Result<Vec<Event>>

/// 全量迭代（逐行 JSON 解析）
fn iter_events(ledger_path: &Path) -> Result<impl Iterator<Item = Result<Event>>>

/// 统计事件数量
fn count_events(ledger_path: &Path) -> Result<usize>
```

**实现细节**:
- 写入：`fs2::FileExt::lock_exclusive()` → `OpenOptions::new().append(true).open()` → `write!(f, "{}\n", json)` → `unlock()`
- 反向扫描：从文件末尾按 4KB 块反向读取，找 `\n` 分割
- 文件不存在时自动创建

**PBT 属性**:
- 单调性：追加 N 个事件后，`count_events() == N`
- 完整性：`iter_events()` 返回的每一行都是合法 JSON 且可反序列化为 Event
- 原子性：并发 100 个线程同时 append，最终 count 等于总写入数
- 往返性：写入事件 E → 读取最后一行 → 等于 E

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-ledger/tests/ledger_test.rs
   - #[test] fn append_and_count()
     → 追加 10 事件 → count == 10
   - #[test] fn append_and_read_last()
     → 追加 5 事件 → read_last_lines(3) 返回最后 3 个
   - #[test] fn iter_all_events()
     → 追加 N 事件 → iter 收集 == N 且顺序正确
   - proptest! fn append_count_monotonic(n in 1..200usize)
     → 追加 n 事件 → count == n
   - proptest! fn append_roundtrip(event in arb_event())
     → 追加 → read_last_lines(1) → 等于原事件
   - #[test] fn concurrent_append_atomicity()
     → 100 线程各追加 10 事件 → count == 1000
   - #[test] fn empty_ledger_returns_zero()
   - #[test] fn corrupted_line_skipped() [ERR-1]
     → 手动写入一行非 JSON → iter_events 跳过该行不 panic
   → cargo test 编译失败

🟢 Green: 实现 append_event, read_last_lines, iter_events, count_events

🔵 Refactor: 提取 flock 逻辑到内部 with_lock() 辅助函数
```

---

#### T04 - Blob 溢出处理

**前置依赖**: T03
**产出**: `ghostcode-ledger/src/blob.rs`
**参考**: `cccc/src/cccc/kernel/ledger.py` blob 逻辑 + `kernel/blobs.py`

**公共 API**:

```rust
/// 阈值：32KB
const BLOB_THRESHOLD: usize = 32 * 1024;

/// 检查 data 是否需要溢出，如需则写入 blob 文件并返回引用
fn maybe_spill_blob(
    blobs_dir: &Path,
    event_id: &str,
    kind: &EventKind,
    data: &serde_json::Value,
) -> Result<serde_json::Value>  // 返回可能被替换的 data（含 blob 引用）

/// 读取 blob 内容
fn read_blob(blobs_dir: &Path, blob_ref: &str) -> Result<String>
```

**实现细节**:
- 仅 `ChatMessage` 类型检查溢出（`data.body` 字段）
- 溢出文件路径：`blobs/chat.<event_id>.txt`
- 溢出后 data 中保留 `{ "_blob_ref": "chat.<event_id>.txt", "body_preview": "<前200字符>" }`

**PBT 属性**:
- 阈值正确：< 32KB 的消息不产生 blob 文件
- 可恢复：溢出的消息通过 `read_blob` 能还原完整内容
- 幂等性：对同一事件多次调用 `maybe_spill_blob` 结果一致

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-ledger/tests/blob_test.rs
   - #[test] fn small_message_no_blob()
     → 1KB body → maybe_spill_blob → 无 blob 文件产生
   - #[test] fn large_message_spills_blob()
     → 50KB body → maybe_spill_blob → blob 文件存在
   - #[test] fn blob_roundtrip()
     → 50KB body → spill → read_blob → 内容相等
   - proptest! fn threshold_boundary(size in 30000..35000usize)
     → 生成 size 字节 body → < 32768 无 blob，>= 32768 有 blob
   - proptest! fn blob_content_integrity(body in "\\PC{0,50000}")
     → spill → read → 逐字节相等（含特殊字符）[补充 PBT]
   - #[test] fn spill_idempotent()
     → 两次 spill 同一事件 → 结果一致
   → cargo test 编译失败

🟢 Green: 实现 maybe_spill_blob, read_blob

🔵 Refactor: 无明显重构点
```

---

### 阶段 C：Daemon 骨架

#### T05 - 单实例锁 + 进程管理

**前置依赖**: T01
**产出**: `ghostcode-daemon/src/lock.rs` + `ghostcode-daemon/src/process.rs`
**参考**: `cccc/src/cccc/util/file_lock.py` + `daemon_main.py`

**公共 API**:

```rust
/// 尝试获取单实例锁，失败说明 daemon 已运行
fn try_acquire_singleton_lock(lock_path: &Path) -> Result<fs2::File>

/// 写入端点描述符
fn write_addr_descriptor(addr_path: &Path, descriptor: &AddrDescriptor) -> Result<()>

/// 读取端点描述符（CLI 端使用）
fn read_addr_descriptor(addr_path: &Path) -> Result<Option<AddrDescriptor>>

/// 清理残留文件（socket + addr + pid）
fn cleanup_stale_files(daemon_dir: &Path) -> Result<()>
```

**目录结构**:
```
~/.ghostcode/
  daemon/
    ghostcoded.lock          -- 单实例锁
    ghostcoded.sock          -- Unix Socket
    ghostcoded.addr.json     -- 端点描述符
    ghostcoded.pid           -- PID 文件
    ghostcoded.log           -- 日志
  groups/<group_id>/
    group.yaml               -- Group 配置
    state/
      ledger/
        ledger.jsonl         -- 事件账本
        ledger.lock          -- 账本写锁
        blobs/               -- 大消息溢出
      read_cursors.json      -- 已读游标
```

**约束**:
- 锁文件使用 `fs2::FileExt::try_lock_exclusive()`（非阻塞）
- daemon 启动时先检查锁 → 锁可用但 socket 文件存在 → `unlink` 残留 socket
- PID 文件写入当前进程 PID
- socket 文件权限设置为 `0o600`（安全考虑，防止其他用户注入）

**PBT 属性**:
- 互斥性：两次 `try_acquire_singleton_lock` 第二次必失败
- 清理正确：`cleanup_stale_files` 后目录下无残留 socket/addr/pid 文件

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-daemon/tests/lock_test.rs
   - #[test] fn singleton_lock_acquired()
     → 获取锁成功
   - #[test] fn singleton_lock_exclusive()
     → 第一次成功 → 第二次失败
   - #[test] fn addr_descriptor_roundtrip()
     → write → read → 相等
   - #[test] fn cleanup_stale_files()
     → 创建假 socket/addr/pid → cleanup → 全部不存在
   - #[test] fn socket_path_length_check() [ASSUM-2]
     → 路径 > 100 字符时使用 /tmp 后备
   → cargo test 编译失败

🟢 Green: 实现 lock/process 模块

🔵 Refactor: 提取路径生成到 DaemonPaths 结构体
```

---

#### T06 - Unix Socket 监听 + JSON-RPC 协议层

**前置依赖**: T02, T05
**产出**: `ghostcode-daemon/src/server.rs` + `ghostcode-daemon/src/protocol.rs`
**参考**: `cccc/src/cccc/daemon/server.py:375-434` + `socket_protocol_ops.py`

**公共 API**:

```rust
/// 启动 daemon 服务（主入口）
async fn serve_forever(config: DaemonConfig) -> Result<()>

/// 处理单个连接（读取请求 → 分发 → 返回响应）
async fn handle_connection(stream: UnixStream, state: Arc<AppState>) -> Result<()>

/// 协议层：从流中读取一个 DaemonRequest
async fn read_request(reader: &mut BufReader<ReadHalf>) -> Result<Option<DaemonRequest>>

/// 协议层：写入一个 DaemonResponse
async fn write_response(writer: &mut WriteHalf, response: &DaemonResponse) -> Result<()>
```

**协议规范**:
- 每个请求/响应是一行 JSON + `\n`
- 每个连接独立（无多路复用）
- 连接断开 → 清理该连接关联的资源

**约束**:
- 使用 `tokio::net::UnixListener`
- 每个连接 spawn 一个 tokio task
- 超时：单个请求处理不超过 30s
- 优雅关闭：收到 SIGTERM/SIGINT → 停止接受新连接 → 等待在途请求完成（最多 5s） → 退出

**PBT 属性**:
- 协议对称：`write_response(read_request(req)) → req` 可往返
- 并发安全：100 个并发连接同时发 ping，全部收到 ok 响应
- 优雅关闭：发 SIGTERM 后不丢失在途请求的响应

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-daemon/tests/server_test.rs
   - #[tokio::test] fn ping_pong()
     → 启动 server → 发 ping → 收到 ok
   - #[tokio::test] fn protocol_roundtrip()
     → write DaemonRequest → read → 内容一致
   - #[tokio::test] fn concurrent_100_pings()
     → 100 并发连接各发 ping → 全部收到响应
   - #[tokio::test] fn oversized_request_rejected() [ERR-2]
     → 发送 2MB JSON → 连接被断开
   - #[tokio::test] fn malformed_json_returns_error() [补充 PBT]
     → 发送非 JSON 数据 → 收到 error 响应，server 不崩溃
   - proptest! fn random_bytes_no_crash(data in prop::collection::vec(any::<u8>(), 0..1024))
     → 发送随机字节 → server 存活
   → cargo test 编译失败

🟢 Green: 实现 serve_forever, handle_connection, read_request, write_response

🔵 Refactor: 提取协议层到独立 protocol.rs 模块

📋 契约测试：导出 JSON Schema 文件 schemas/daemon-request.json, schemas/daemon-response.json
   供 T18 (TS IPC) 使用
```

---

#### T07 - 请求分发框架 (dispatch)

**前置依赖**: T06
**产出**: `ghostcode-daemon/src/dispatch.rs`
**参考**: `cccc/src/cccc/daemon/request_dispatch_ops.py`

**Phase 1 需要的 Op 列表（20 个）**:

```
// 核心
ping, shutdown

// Group 管理
group_create, group_show, group_start, group_stop, group_delete, group_set_state, groups

// Actor 管理
actor_add, actor_list, actor_start, actor_stop, actor_remove

// 消息
send, reply, inbox_list, inbox_mark_read, inbox_mark_all_read

// Headless
headless_status, headless_set_status
```

**实现方式**:

```rust
/// 分发请求到对应处理器
async fn dispatch(state: &AppState, req: DaemonRequest) -> DaemonResponse {
    match req.op.as_str() {
        "ping" => handle_ping(state),
        "shutdown" => handle_shutdown(state),
        "group_create" => handle_group_create(state, req.args),
        // ... 每个 op 一个 handler 函数
        _ => DaemonResponse::error("unknown_op", format!("未知操作: {}", req.op)),
    }
}
```

**约束**:
- 每个 handler 是独立的 `async fn`，接收 `&AppState` + `serde_json::Value`
- handler 返回 `Result<serde_json::Value, DaemonError>`
- dispatch 层统一包装为 `DaemonResponse`
- 未知 op 返回明确错误（不 panic）

**PBT 属性**:
- 完备性：上述 20 个 op 字符串都能分发到对应 handler
- 未知 op 安全：任意随机字符串 op 都返回 `ok: false` 而非 panic

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-daemon/tests/dispatch_test.rs
   - #[tokio::test] fn dispatch_all_known_ops()
     → 遍历 20 个 op 字符串 → 全部不返回 "unknown_op" 错误
   - proptest! fn unknown_op_returns_error(op in "[a-z_]{1,50}")
     → 排除已知 op → 返回 ok: false, error.code == "unknown_op"
   - #[tokio::test] fn ping_returns_version() [ERR-3]
     → ping 响应包含 version 和 has_unread 字段
   → cargo test 编译失败

🟢 Green: 实现 dispatch 函数 + ping/shutdown handler
   （其他 handler 返回 "not_implemented" 占位，后续任务逐步实现）

🔵 Refactor: 使用宏或 phf_map 替代 match 大分支（可选）
```

---

### 阶段 D：Agent 管理

#### T08 - Group 数据模型 + 持久化

**前置依赖**: T02, T03
**产出**: `ghostcode-daemon/src/group.rs`
**参考**: `cccc/src/cccc/kernel/group.py`

**公共 API**:

```rust
/// 创建 Group（生成 group_id，创建目录结构，写入 group.yaml，写入 GroupCreate 事件）
fn create_group(groups_dir: &Path, title: &str) -> Result<GroupInfo>

/// 加载 Group（从 group.yaml 读取）
fn load_group(group_dir: &Path) -> Result<GroupInfo>

/// 列出所有 Group
fn list_groups(groups_dir: &Path) -> Result<Vec<GroupInfo>>

/// 删除 Group（清理目录）
fn delete_group(groups_dir: &Path, group_id: &str) -> Result<()>

/// 设置 Group 状态（写入 GroupSetState 事件）
fn set_group_state(group: &mut GroupInfo, state: GroupState, ledger: &LedgerWriter) -> Result<()>
```

**目录结构**（每个 Group）:
```
~/.ghostcode/groups/<group_id>/
  group.yaml               -- 配置：title, actors[], state, created_at
  state/
    ledger/
      ledger.jsonl
      ledger.lock
      blobs/
    read_cursors.json
```

**约束**:
- group_id 格式：`g-<8位随机hex>`（如 `g-a1b2c3d4`）
- group.yaml 使用 serde_yaml 序列化
- 创建 Group 时同步创建完整目录结构
- Group 状态变更必须同时更新 yaml 和写入事件

**PBT 属性**:
- 往返性：`create_group → load_group` 得到相同数据
- 删除彻底：`delete_group` 后目录不存在
- 状态一致：group.yaml 中的 state 与最后一个 GroupSetState 事件一致

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-daemon/tests/group_test.rs
   - #[test] fn create_and_load_group()
     → create → load → title 和 state 一致
   - #[test] fn list_groups()
     → 创建 3 个 group → list 返回 3 个
   - #[test] fn delete_group_removes_dir()
     → create → delete → 目录不存在
   - #[test] fn set_group_state()
     → create → set Active → load → state == Active
   - proptest! fn create_load_roundtrip(title in "[a-zA-Z ]{1,50}")
     → create(title) → load → title 相等
   - #[test] fn delete_group_cleans_blobs() [AMB-3]
     → create → 写入 blob 文件 → delete → blobs 目录不存在
   → cargo test 编译失败

🟢 Green: 实现 group 模块

🔵 Refactor: 提取 YAML 读写到内部 persistence 辅助
```

---

#### T09 - Actor 注册/发现/移除

**前置依赖**: T08
**产出**: `ghostcode-daemon/src/actor.rs`
**参考**: `cccc/src/cccc/kernel/actors.py`

**公共 API**:

```rust
/// 添加 Actor 到 Group（写入 ActorAdd 事件，更新 group.yaml）
fn add_actor(group: &mut GroupInfo, actor: ActorInfo, ledger: &LedgerWriter) -> Result<()>

/// 查找 Actor
fn find_actor(group: &GroupInfo, actor_id: &str) -> Option<&ActorInfo>

/// 查找 Foreman（role == Foreman 的 Actor）
fn find_foreman(group: &GroupInfo) -> Option<&ActorInfo>

/// 列出所有 Actor
fn list_actors(group: &GroupInfo) -> &[ActorInfo]

/// 移除 Actor（写入 ActorRemove 事件）
fn remove_actor(group: &mut GroupInfo, actor_id: &str, ledger: &LedgerWriter) -> Result<()>

/// 生成 Actor ID
fn generate_actor_id(prefix: &str, runtime: &RuntimeKind) -> String
```

**约束**:
- actor_id 格式：`<prefix>-<runtime>-<4位hex>`（如 `dev-claude-a1b2`）
- 每个 Group 最多 1 个 Foreman
- 添加第二个 Foreman 时返回错误
- Actor 信息持久化到 group.yaml 的 actors 数组

**PBT 属性**:
- 唯一性：同一 Group 内不存在重复 actor_id
- 可发现：`add_actor` 后 `find_actor` 必能找到
- 移除干净：`remove_actor` 后 `find_actor` 返回 None
- Foreman 唯一：任何操作序列后，Group 内 Foreman 数量 <= 1

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-daemon/tests/actor_test.rs
   - #[test] fn add_and_find_actor()
   - #[test] fn add_three_agents()
     → 添加 claude(Foreman) + codex(Peer) + gemini(Peer) → list 返回 3 个
   - #[test] fn find_foreman()
     → 添加 claude(Foreman) → find_foreman 返回 claude
   - #[test] fn remove_actor()
     → add → remove → find 返回 None
   - #[test] fn duplicate_actor_id_rejected()
     → add 同 id 两次 → 第二次返回错误
   - #[test] fn second_foreman_rejected()
     → add Foreman → add 第二个 Foreman → 错误
   - proptest! fn actor_id_always_unique(
       ops in prop::collection::vec(arb_actor_op(), 1..50))
     → 执行随机 add/remove 序列 → 始终无重复 id
   → cargo test 编译失败

🟢 Green: 实现 actor 模块

🔵 Refactor: 将 actor_id 生成提取为独立函数
```

---

#### T10 - Actor 生命周期（启停 + 异常检测）

**前置依赖**: T09
**产出**: `ghostcode-daemon/src/lifecycle.rs` + `ghostcode-daemon/src/runner.rs`
**参考**: `cccc/src/cccc/runners/headless.py` + `daemon/actors/actor_lifecycle_ops.py`

**公共 API**:

```rust
/// Headless Actor 状态机
enum HeadlessStatus { Idle, Working, Waiting, Stopped }

/// 启动 Actor（写入 ActorStart 事件，初始化 HeadlessSession）
async fn start_actor(state: &AppState, group_id: &str, actor_id: &str) -> Result<()>

/// 停止 Actor（写入 ActorStop 事件，清理 session）
async fn stop_actor(state: &AppState, group_id: &str, actor_id: &str) -> Result<()>

/// 获取 Headless 状态
fn get_headless_status(state: &AppState, group_id: &str, actor_id: &str) -> Result<HeadlessStatus>

/// 设置 Headless 状态（Agent 主动报告）
fn set_headless_status(state: &AppState, group_id: &str, actor_id: &str, status: HeadlessStatus) -> Result<()>

/// Daemon 启动时恢复运行中的 Actor
async fn restore_running_actors(state: &AppState) -> Result<()>
```

**AppState 中的 Actor 运行时数据**:
```rust
struct AppState {
    groups_dir: PathBuf,
    daemon_dir: PathBuf,
    // 运行时（内存中）
    headless_sessions: Arc<RwLock<HashMap<(String, String), HeadlessSession>>>,
    // 事件广播
    event_tx: tokio::sync::broadcast::Sender<Event>,
}
```

**约束**:
- Phase 1 仅实现 Headless 模式（PTY 延后到 Phase 1.5）
- Headless 状态转换：`Idle → Working → Waiting → Idle`（循环）或 `→ Stopped`
- `restore_running_actors`：扫描所有 group.yaml，对 state != Stopped 的 Group 恢复其 actors
- Actor 异常检测通过 Headless 心跳超时实现：Agent 超过 60s 未报告状态 → 视为异常

**PBT 属性**:
- 状态一致：`start_actor` 后 `get_headless_status` 返回 `Idle`
- 停止幂等：多次 `stop_actor` 不报错
- 恢复正确：写入 N 个运行中 Actor → 重启 → `restore_running_actors` 后内存中有 N 个 session

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-daemon/tests/lifecycle_test.rs
   - #[tokio::test] fn start_actor_sets_idle()
     → start → get_status == Idle
   - #[tokio::test] fn stop_actor_cleans_session()
     → start → stop → get_status 返回 None 或 Stopped
   - #[tokio::test] fn stop_idempotent()
     → start → stop → stop → 不报错
   - #[tokio::test] fn status_transitions()
     → start(Idle) → set(Working) → set(Waiting) → set(Idle) → 全部成功
   - #[tokio::test] fn restore_running_actors()
     → 创建 Group + 3 Actor(running) → 构建新 AppState → restore → 内存中 3 个 session
   - #[tokio::test] fn heartbeat_timeout_detection()
     → start → 等待 > 60s → Actor 标记为异常（可用 tokio::time::advance 模拟）
   → cargo test 编译失败

🟢 Green: 实现 lifecycle + runner 模块

🔵 Refactor: 提取 HeadlessSession 到独立文件
```

---

### 阶段 E：消息投递

#### T11 - 消息发送 + 事件写入

**前置依赖**: T03, T09
**产出**: `ghostcode-daemon/src/messaging/send.rs`
**参考**: `cccc/src/cccc/daemon/messaging/chat_ops.py`

**公共 API**:

```rust
/// 发送消息（写入 ChatMessage 事件 → 入投递队列）
async fn send_message(
    state: &AppState,
    group_id: &str,
    sender_id: &str,
    recipients: Vec<String>,  // actor_id 列表，空 = 广播
    body: String,
    reply_to: Option<String>, // 引用的 event_id
) -> Result<Event>

/// 回复消息（send_message 的便捷封装，自动填 reply_to）
async fn reply_message(
    state: &AppState,
    group_id: &str,
    sender_id: &str,
    reply_to_event_id: &str,
    body: String,
) -> Result<Event>
```

**约束**:
- sender_id 必须是 Group 内已注册的 Actor
- recipients 为空时广播给 Group 内除 sender 外的所有 Actor
- body 超过 32KB 时触发 blob 溢出（调用 T04）
- 写入事件后立即推入投递队列（T13 的 DeliveryThrottle）
- Group 状态为 Paused 时，消息写入账本但不投递（累积在 inbox）

**PBT 属性**:
- 持久化保证：`send_message` 成功返回 → 事件一定在账本中
- 广播正确：N 个 Actor 的 Group，广播后 N-1 个 Actor 的 inbox 中有该消息
- 大消息处理：发送 50KB 消息 → blob 文件存在 → 读取 blob 还原完整内容

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-daemon/tests/send_test.rs
   - #[tokio::test] fn send_message_persisted()
     → send → iter_events 包含该消息
   - #[tokio::test] fn send_to_specific_recipient()
     → claude → codex → codex inbox 有消息，gemini inbox 无
   - #[tokio::test] fn broadcast_to_all()
     → claude 广播 → codex 和 gemini inbox 都有
   - #[tokio::test] fn reply_links_to_original()
     → send → reply → reply 事件的 data.reply_to == 原 event_id
   - #[tokio::test] fn large_message_blob_spill()
     → 发送 50KB → blob 文件存在
   - #[tokio::test] fn paused_group_no_delivery()
     → set_state(Paused) → send → 消息在账本但 has_unread 不更新
   → cargo test 编译失败

🟢 Green: 实现 send_message, reply_message

🔵 Refactor: 提取 recipients 解析逻辑
```

---

#### T12 - Inbox 读取 + 已读游标

**前置依赖**: T03, T11
**产出**: `ghostcode-daemon/src/messaging/inbox.rs`
**参考**: `cccc/src/cccc/kernel/inbox.py:17-80`

**公共 API**:

```rust
/// 获取 Actor 的未读消息列表
fn unread_messages(
    group: &GroupInfo,
    actor_id: &str,
    limit: usize,
) -> Result<Vec<Event>>

/// 标记已读（移动游标到指定 event_id）
fn mark_read(
    group: &GroupInfo,
    actor_id: &str,
    event_id: &str,
) -> Result<()>

/// 全部标记已读
fn mark_all_read(
    group: &GroupInfo,
    actor_id: &str,
) -> Result<()>

/// 获取未读消息数
fn unread_count(
    group: &GroupInfo,
    actor_id: &str,
) -> Result<usize>

/// ACK 重要消息（写入 ChatAck 事件）
fn ack_message(
    group: &GroupInfo,
    actor_id: &str,
    event_id: &str,
    ledger: &LedgerWriter,
) -> Result<()>
```

**已读游标存储**:
- 文件：`<group_dir>/state/read_cursors.json`
- 格式：`{ "<actor_id>": { "event_id": "xxx", "ts": "2026-..." } }`
- 读写用 flock 保护

**约束**:
- 未读 = 账本中 ts > cursor.ts 的 ChatMessage 事件且 recipients 包含该 actor_id
- `mark_read` 写入 ChatRead 事件 + 更新 read_cursors.json
- inbox_list 按时间倒序返回

**PBT 属性**:
- 游标单调：mark_read 后游标只会前进不会后退
- 已读不重现：mark_read(E) 后 unread_messages 不包含 E
- 计数一致：`unread_count == len(unread_messages)`

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-daemon/tests/inbox_test.rs
   - #[test] fn unread_messages_returns_new()
     → 发送 3 条 → unread 返回 3 条
   - #[test] fn mark_read_advances_cursor()
     → 发送 3 条 → mark_read(第2条) → unread 返回 1 条
   - #[test] fn mark_all_read()
     → 发送 5 条 → mark_all → unread == 0
   - #[test] fn unread_count_consistent()
     → unread_count == unread_messages.len()
   - proptest! fn cursor_monotonic(
       reads in prop::collection::vec(1..100u64, 1..20))
     → 按随机顺序 mark_read → 最终游标 == max(reads) [ASSUM-1]
   - #[test] fn cursor_uses_seq_not_ts() [ASSUM-1]
     → 验证游标比较基于 seq 字段而非 ts
   - #[test] fn ack_message_creates_event()
     → ack → 账本中有 ChatAck 事件
   → cargo test 编译失败

🟢 Green: 实现 inbox 模块

🔵 Refactor: 内存游标 + 异步刷盘分离
```

---

#### T13 - 投递引擎 (DeliveryThrottle)

**前置依赖**: T10, T11
**产出**: `ghostcode-daemon/src/messaging/delivery.rs`
**参考**: `cccc/src/cccc/daemon/messaging/delivery.py:247-461`

**公共 API**:

```rust
/// 投递引擎（后台 tokio task）
struct DeliveryEngine {
    /// 将消息加入投递队列
    fn enqueue(&self, group_id: &str, event: &Event, recipients: &[String])

    /// 启动投递循环（每秒 tick）
    async fn run(&self, state: Arc<AppState>)
}

/// 投递节流器（per-actor）
struct DeliveryThrottle {
    queue: VecDeque<PendingMessage>,
    last_delivery_at: Option<Instant>,
    last_attempt_at: Option<Instant>,
}
```

**投递规则**:
```
首次投递（last_delivery_at == None）:
  → 从未尝试过: 立即投递
  → 已尝试过: elapsed_attempt >= 5s 才允许

后续投递:
  → elapsed_delivery >= min_interval (默认 0s)
  → 且 elapsed_attempt >= 5s

失败: requeue_front() 放回队列头部
```

**投递方式（Headless 模式）**:
- 不直接投递，消息写入账本后 Agent 通过 `inbox_list` 主动拉取
- DeliveryEngine 的职责：维护"待通知"队列，确保 Agent 知道有新消息
- 通过 `event_tx` broadcast channel 通知在线的 Agent

**约束**:
- 投递循环频率：每秒 1 次 tick
- per-actor 队列，互不影响
- 队列最大深度：1000 条（超出丢弃最旧的）

**PBT 属性**:
- 节流正确：5s 内对同一 Actor 的投递尝试 <= 1 次
- 队列有界：队列长度永远 <= 1000
- 失败重试：投递失败的消息回到队列头部（LIFO 重试）

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-daemon/tests/delivery_test.rs
   - #[tokio::test] fn enqueue_and_flush()
     → enqueue 消息 → tick → has_unread 被设置
   - #[tokio::test] fn throttle_5s_interval()
     → enqueue 2 条 → tick → 第 1 条通知 → 立即 tick → 第 2 条未通知
     → advance 5s → tick → 第 2 条通知
   - #[tokio::test] fn queue_bounded_1000()
     → enqueue 2000 条 → 队列长度 == 1000 → 最旧 1000 条被丢弃
   - #[tokio::test] fn ping_includes_has_unread() [AMB-1]
     → send 消息给 actor → actor ping → has_unread == true
     → actor inbox_list → actor ping → has_unread == false
   - proptest! fn throttle_rate_bounded(
       msgs in prop::collection::vec(arb_message(), 1..100))
     → 在 5s 窗口内对同一 actor 的通知次数 <= 1
   → cargo test 编译失败

🟢 Green: 实现 delivery 模块

🔵 Refactor: 提取 ThrottleState 为独立结构体
```

---

### 阶段 F：MCP 工具层

#### T14 - stdio JSON-RPC 2.0 Server 框架

**前置依赖**: T02
**产出**: `ghostcode-mcp/src/server.rs` + `ghostcode-mcp/src/jsonrpc.rs`
**参考**: `cccc/src/cccc/ports/mcp/main.py:228-241`

**公共 API**:

```rust
/// MCP Server 主入口（stdio 模式）
async fn serve_stdio(group_id: &str, actor_id: &str, daemon_addr: &Path) -> Result<()>

/// JSON-RPC 2.0 请求
struct JsonRpcRequest {
    jsonrpc: String,  // "2.0"
    id: serde_json::Value,
    method: String,
    params: serde_json::Value,
}

/// JSON-RPC 2.0 响应
struct JsonRpcResponse {
    jsonrpc: String,
    id: serde_json::Value,
    result: Option<serde_json::Value>,
    error: Option<JsonRpcError>,
}
```

**约束**:
- 从 stdin 逐行读取 JSON-RPC 请求
- 处理后向 stdout 写入 JSON-RPC 响应
- MCP 标准方法：`initialize`, `tools/list`, `tools/call`
- 身份通过环境变量注入：`GHOSTCODE_GROUP_ID`, `GHOSTCODE_ACTOR_ID`
- 内部通过 Unix socket 连接到 Daemon 转发请求

**PBT 属性**:
- 协议合规：所有响应都包含 `jsonrpc: "2.0"` 和对应的 `id`
- 未知方法安全：返回 JSON-RPC error code -32601 (Method not found)

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-mcp/tests/jsonrpc_test.rs
   - #[tokio::test] fn initialize_handshake()
     → 发送 initialize → 收到 capabilities
   - #[tokio::test] fn tools_list_returns_8()
     → 发送 tools/list → 返回 8 个工具定义
   - #[test] fn response_always_has_jsonrpc_and_id()
     → 构造请求 → 响应必含 jsonrpc:"2.0" + 对应 id
   - #[tokio::test] fn unknown_method_error()
     → 发送 "foo/bar" → error code == -32601
   - proptest! fn response_id_matches_request(id in any::<u64>())
     → 请求 id=N → 响应 id=N
   → cargo test 编译失败

🟢 Green: 实现 MCP server 框架

🔵 Refactor: 提取 JSON-RPC 协议层为独立 jsonrpc.rs
```

---

#### T15 - 核心 MCP 工具实现（8 个）

**前置依赖**: T14, T07（dispatch 框架）
**产出**: `ghostcode-mcp/src/tools/`

**工具列表**:

| # | 工具名 | 对应 Daemon Op | 参数 | 返回 |
|---|--------|---------------|------|------|
| 1 | `ghostcode_message_send` | `send` | to, body, reply_to? | event_id |
| 2 | `ghostcode_inbox_list` | `inbox_list` | limit? | messages[] |
| 3 | `ghostcode_inbox_mark_read` | `inbox_mark_read` | event_id | ok |
| 4 | `ghostcode_inbox_mark_all_read` | `inbox_mark_all_read` | (无) | ok |
| 5 | `ghostcode_actor_list` | `actor_list` | (无) | actors[] |
| 6 | `ghostcode_actor_start` | `actor_start` | actor_id | ok |
| 7 | `ghostcode_actor_stop` | `actor_stop` | actor_id | ok |
| 8 | `ghostcode_group_info` | `group_show` | (无) | group |

**约束**:
- 每个工具函数：解析参数 → 构造 DaemonRequest → 通过 socket 发给 Daemon → 解析响应 → 返回 JSON-RPC 结果
- group_id 和 actor_id 从环境变量获取，不作为工具参数
- 工具描述使用英文（MCP 标准）

**PBT 属性**:
- 完备性：`tools/list` 返回的工具数 == 8
- 参数校验：缺少必填参数时返回 JSON-RPC error，不 panic

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-mcp/tests/tools_test.rs
   - #[tokio::test] fn message_send_tool()
     → tools/call ghostcode_message_send {to:"codex", body:"hello"}
     → 验证 daemon 账本中有该消息
   - #[tokio::test] fn inbox_list_tool()
     → 先 send 消息 → tools/call ghostcode_inbox_list → 返回消息
   - #[tokio::test] fn actor_list_tool()
     → 注册 3 agent → tools/call ghostcode_actor_list → 返回 3 个
   - #[tokio::test] fn missing_param_error()
     → tools/call ghostcode_message_send {} (缺 body) → error
   - 每个工具一个基本测试（共 8 个）
   → cargo test 编译失败

🟢 Green: 实现 8 个工具的 handler

🔵 Refactor: 提取公共的 daemon 调用逻辑到 DaemonClient
```

---

### 阶段 G：TS Plugin 薄壳

#### T16 - Plugin 项目初始化

**前置依赖**: 无（可与 Rust 并行）
**产出**: `src/plugin/` 目录

```
src/plugin/
  package.json
  tsconfig.json
  src/
    index.ts            -- Plugin 入口
    daemon.ts           -- Daemon 管理
    ipc.ts              -- IPC 通信
    hooks/
      index.ts          -- Hook 注册
  .claude/
    settings.json       -- Plugin 配置声明
```

**约束**:
- package.json name: `ghostcode`
- 使用 `pnpm` 作为包管理器（禁止 npm）
- TypeScript strict mode
- 构建工具：`tsup`
- 目标：ESM 模块

**TDD 步骤**:
```
🔴 Red:   无（脚手架任务无测试）
🟢 Green: 创建 package.json + tsconfig + 目录结构
🔵 Check: pnpm build 零错误
```

---

#### T17 - Daemon 生命周期管理

**前置依赖**: T16, T05
**产出**: `src/plugin/src/daemon.ts`

**公共 API**:

```typescript
/// 确保 Daemon 运行（不运行则启动）
async function ensureDaemon(): Promise<AddrDescriptor>

/// 停止 Daemon
async function stopDaemon(): Promise<void>

/// 心跳检测（每 30s ping 一次）
function startHeartbeat(addr: AddrDescriptor): () => void  // 返回 stop 函数

/// 获取 Rust 二进制路径
function getDaemonBinaryPath(): string
```

**启动流程**:
```
1. 读取 ~/.ghostcode/daemon/ghostcoded.addr.json
2. 如果存在 → ping 测试 → 成功则复用
3. 如果不存在或 ping 失败 → spawn 新 daemon 子进程
4. 等待 addr.json 出现（最多 5s）
5. ping 确认 → 返回 AddrDescriptor
```

**约束**:
- 使用 `child_process.spawn` 启动 Daemon（detached: true）
- Daemon 二进制路径：`~/.ghostcode/bin/ghostcoded`
- 心跳失败 3 次 → 尝试重启 Daemon

**TDD 步骤**:
```
🔴 Red: src/plugin/tests/daemon.test.ts (vitest)
   - test('ensureDaemon starts daemon if not running')
   - test('ensureDaemon reuses existing daemon')
   - test('heartbeat detects daemon crash')
   - test('version mismatch triggers restart') [ERR-3]
   - test('concurrent ensureDaemon calls safe') [RACE-2]
   → pnpm test 失败

🟢 Green: 实现 daemon.ts

🔵 Refactor: 提取重试逻辑到 retry() 工具函数
```

---

#### T18 - IPC 桥接层

**前置依赖**: T17
**产出**: `src/plugin/src/ipc.ts`

**公共 API**:

```typescript
/// 发送请求到 Daemon 并等待响应
async function callDaemon(op: string, args?: Record<string, unknown>): Promise<DaemonResponse>

/// 创建 Unix socket 连接
function createConnection(socketPath: string): Promise<net.Socket>
```

**约束**:
- 使用 Node.js `net.connect` 连接 Unix socket
- 协议：发送一行 JSON + `\n`，读取一行 JSON + `\n`
- 超时：单次请求 10s
- 连接池：保持 1 个持久连接，断开后自动重连

**PBT 属性**:
- 延迟：`callDaemon("ping")` 的 p99 < 100ms

**TDD 步骤**:
```
🔴 Red: src/plugin/tests/ipc.test.ts (vitest)
   - test('callDaemon ping returns ok')
   - test('callDaemon timeout after 10s')
   - test('auto-reconnect on connection drop')
   - test('p99 latency under 100ms', async () => {
       const times = [];
       for (let i = 0; i < 1000; i++) {
         const start = performance.now();
         await callDaemon('ping');
         times.push(performance.now() - start);
       }
       times.sort((a,b) => a-b);
       expect(times[989]).toBeLessThan(100); // p99
     })
   → pnpm test 失败

🟢 Green: 实现 ipc.ts

🔵 Refactor: 无
```

📋 **契约测试**: ipc.ts 的 DaemonRequest/Response 类型必须与
   T06 导出的 schemas/daemon-request.json, schemas/daemon-response.json 一致

---

### 阶段 H：集成 + 分发

#### T19 - 三 Agent 端到端集成测试

**前置依赖**: T01-T18 全部
**产出**: `tests/integration/` 目录

**测试场景**:

```
场景 1：基本消息流
  1. 启动 Daemon
  2. 创建 Group
  3. 注册 3 个 Actor：claude(Foreman), codex(Peer), gemini(Peer)
  4. claude → codex: "分析后端模块"
  5. codex 通过 inbox_list 收到消息
  6. codex → claude: reply "分析完成"
  7. claude 通过 inbox_list 收到回复
  8. 验证：所有消息在账本中可查

场景 2：广播
  1. claude 发送广播消息（recipients 为空）
  2. codex 和 gemini 都收到
  3. 验证：两者 inbox 中都有该消息

场景 3：持久化恢复
  1. 发送 10 条消息
  2. 关闭 Daemon
  3. 重启 Daemon
  4. inbox_list 查询 → 消息完整

场景 4：Agent 异常退出通知
  1. 注册 3 个 Actor
  2. 模拟 codex 异常退出（set_headless_status → Stopped）
  3. 验证 Foreman (claude) 收到 SystemNotify

场景 5：Group 状态影响投递
  1. set_group_state → Paused
  2. 发送消息 → 写入账本但不投递
  3. set_group_state → Active
  4. 消息被投递
```

**TDD 步骤**:
```
这是 BDD/ATDD 层级的测试，使用 Rust 集成测试：

🔴 Red: tests/integration/three_agents_test.rs
   - 每个场景一个 #[tokio::test] 函数
   - 场景 1-5 全部先写失败测试
   → cargo test --test three_agents_test 全部失败

🟢 Green: 此时所有模块已实现，测试应直接通过
   如有失败 → 修复对应模块的 bug

🔵 Refactor: 提取测试辅助：setup_daemon(), create_three_agents(), cleanup()
```

---

#### T20 - 跨平台构建 + Plugin 打包

**前置依赖**: T19
**产出**: CI 配置 + 打包脚本

**构建目标**:
- `aarch64-apple-darwin` (macOS ARM)
- `x86_64-apple-darwin` (macOS Intel)
- `x86_64-unknown-linux-gnu` (Linux)

**Plugin 打包**:
```
ghostcode-plugin/
  package.json
  dist/
    index.js            -- tsup 编译产出
  bin/
    ghostcoded-darwin-arm64
    ghostcoded-darwin-x64
    ghostcoded-linux-x64
  .claude/
    settings.json
```

**安装流程**:
- `claude plugin install ghostcode` → 下载 Plugin
- Plugin 首次运行时检测平台 → 选择对应二进制 → 复制到 `~/.ghostcode/bin/ghostcoded`

**TDD 步骤**:
```
🔴 Red: .github/workflows/ci.yml
   - job: build-rust
     → cargo build --release 三个 target
     → cargo test --workspace
     → cargo clippy -- -D warnings
   - job: build-ts
     → pnpm install && pnpm build && pnpm test
   - job: mutation (定期)
     → cargo mutants --timeout 60

🟢 Green: CI 全绿

🔵 Release: GitHub Release 附带三平台二进制 + npm 包
```

---

## 依赖关系图

```
T01 ──→ T02 ──→ T03 ──→ T04
  │       │       │
  │       │       ├──→ T08 ──→ T09 ──→ T10
  │       │       │                      │
  │       │       └──→ T11 ──→ T12 ──→ T13
  │       │
  │       └──→ T06 ──→ T07
  │             │
  │             └──→ T14 ──→ T15
  │
  └──→ T05 ──→ T06

T16 ──→ T17 ──→ T18   （可与 Rust 侧并行）

T01-T18 全部 ──→ T19 ──→ T20
```

## 关键并行点

- **T01-T04**（账本）与 **T16**（TS 初始化）可并行
- **T14-T15**（MCP）与 **T08-T13**（Agent+消息）可并行（都依赖 T02+T06）
- **T17-T18**（TS Daemon 管理）需等 T05-T06（Daemon 骨架）完成

---

## 代码量估计

| 阶段 | 任务 | Rust 行数 | TS 行数 |
|------|------|----------|---------|
| A | T01-T02 | ~400 | 0 |
| B | T03-T04 | ~500 | 0 |
| C | T05-T07 | ~800 | 0 |
| D | T08-T10 | ~900 | 0 |
| E | T11-T13 | ~700 | 0 |
| F | T14-T15 | ~600 | 0 |
| G | T16-T18 | 0 | ~500 |
| H | T19-T20 | ~400 | ~200 |
| **合计** | | **~4300** | **~700** |

总计约 **5000 行**代码。

---

## 歧义消除记录（Gemini 审计 2026-02-28）

以下是通过双模型审计发现并已消除的歧义、隐含假设和竞态条件。
每条记录包含：问题 → 决策 → 影响的任务。

### 已消除的 Blocker

#### [AMB-1] Headless 通知机制（影响 T13）

**问题**: Headless 模式下 Agent 怎么知道有新消息？任务清单中说"通过 `event_tx` broadcast channel 通知"但未说明具体协议。

**决策**: 采用 **轮询 + 提示** 双机制（与 CCCC 一致）：
- **主路径**：Agent 通过 MCP 工具 `ghostcode_inbox_list` 主动轮询（建议间隔 2-5s）
- **辅助路径**：Daemon 在 `ping` 响应中附加 `has_unread: true` 标志，提示 Agent 应立即拉取
- **不使用** WebSocket / long-polling / SSE（Phase 1 保持简单）

**T13 补充约束**:
```
- DeliveryEngine 在 Headless 模式下不做"推送投递"
- 职责简化为：维护 per-actor 未读计数 + 在 ping 响应中设置 has_unread 标志
- 节流逻辑保留用于 Phase 1.5 的 PTY 模式
```

---

### 已消除的 Warning

#### [AMB-2] scope_key 生成逻辑（影响 T02）

**问题**: Event.scope_key 未定义生成规则，多项目并行时可能消息污染。

**决策**: Phase 1 中 `scope_key` 固定为空字符串 `""`，不做多 scope 支持。
- 原因：Phase 1 MVP 聚焦单 Group 单 Scope 场景
- 多 Scope 支持延后到 Phase 2（与 ccg-workflow 的 workdir 路由结合）

**T02 补充约束**:
```
- scope_key: String = "" （Phase 1 固定空串）
- Event 序列化时 scope_key 字段保留（向前兼容）
```

#### [AMB-3] Blob 生命周期管理（影响 T04）

**问题**: Blob 文件无清理策略，长期运行产生孤儿文件。

**决策**: Blob 随 Group 删除一起清理。
- `delete_group` (T08) 删除 Group 目录时递归删除 `blobs/` 子目录
- Phase 1 不做独立的 blob GC（消息量可控）

**T04 补充约束**:
```
- Blob 文件存储在 <group_dir>/state/ledger/blobs/ 下
- delete_group 时递归删除整个 group 目录（包含 blobs）
- 不需要独立的 blob 垃圾回收
```

---

### 竞态条件消除

#### [RACE-1] 多进程竞争更新 read_cursors.json（影响 T12）

**问题**: 多个请求并发更新同一 Actor 的已读游标可能数据丢失。

**决策**: 所有 `read_cursors.json` 更新通过 Daemon 单写者串行化。
- Daemon 是唯一写入者（单写者原则 HC-2）
- 游标更新在 Daemon 进程内通过 `RwLock` 保护的内存状态管理
- 持久化到文件时使用 flock（与账本写入共用锁策略）

**T12 补充约束**:
```
- read_cursors 在内存中用 HashMap<(GroupId, ActorId), CursorState> 管理
- 更新时先更新内存 → 再异步刷盘到 read_cursors.json
- 刷盘使用 flock 保护
- 游标只能前进不能后退（单调性约束）
```

#### [RACE-2] Plugin 并发启动 Daemon（影响 T17）

**问题**: 多个 Claude Code 窗口同时启动可能导致竞争 spawn。

**决策**: 利用 T05 的单实例锁 + TS 侧重试退避。
- Daemon 使用 `fs2::try_lock_exclusive` 保证全局唯一
- TS Plugin 的 `ensureDaemon()` 流程：
  1. 尝试 ping 已有 Daemon → 成功则复用
  2. ping 失败 → spawn 新进程
  3. spawn 后等待 addr.json 出现（100ms 轮询，最多 5s）
  4. 若第二个 Plugin 在 spawn 后发现锁已被占 → 回到 step 1 ping 即可

**T17 补充约束**:
```
- ensureDaemon 无需额外锁，因为 Daemon 侧的 fs2 锁保证互斥
- spawn 后轮询 addr.json：间隔 100ms，最多 50 次（5s）
- 轮询成功后再 ping 确认，ping 失败则抛出错误（不无限重试）
```

---

### 隐含假设显式化

#### [ASSUM-1] 已读游标应使用序列号而非时间戳

**问题**: Gemini 指出时钟不同步可能导致消息遗漏。

**决策**: 使用 **事件序列号（单调递增）** 作为游标，时间戳仅做展示。
- 序列号 = 该事件在账本中的行号（从 1 开始）
- 追加事件时由 Daemon 分配（内存中维护 `next_seq: AtomicU64`）
- 重启时从账本行数恢复

**T02 补充**: Event 结构体增加 `seq: u64` 字段：
```rust
struct Event {
    v: u8,
    seq: u64,              // 新增：单调递增序列号
    id: String,
    ts: String,
    kind: EventKind,
    group_id: String,
    scope_key: String,
    by: String,
    data: serde_json::Value,
}
```

**T12 补充**: 已读游标改为基于 seq：
```
- read_cursors.json 格式：{ "<actor_id>": { "seq": 42, "event_id": "xxx", "ts": "..." } }
- 未读判定：event.seq > cursor.seq 且 recipients 包含该 actor
- 游标单调性：新 seq 必须 > 当前 seq，否则忽略
```

#### [ASSUM-2] Unix Socket 路径长度限制

**问题**: macOS 限制 Unix socket 路径 104 字符，`~/.ghostcode/daemon/ghostcoded.sock` 可能超限。

**决策**: 启动时校验路径长度。
- `~/.ghostcode/daemon/ghostcoded.sock` 典型长度约 45-55 字符（安全范围）
- 但如果用户 HOME 路径极深可能超限

**T05 补充约束**:
```
- Daemon 启动时校验 socket 路径长度 <= 100 字符
- 超限时使用 /tmp/ghostcoded-<uid>.sock 作为后备路径
- 后备路径写入 addr.json 的 path 字段，Client 通过 addr.json 发现
```

#### [ASSUM-3] 仅支持本地文件系统

**问题**: flock 在 NFS/SMB 上可能失效。

**决策**: 显式声明仅支持本地文件系统。

**CLAUDE.md 补充**:
```
GhostCode 数据目录（~/.ghostcode/）必须位于本地文件系统（APFS/ext4/HFS+）。
不支持网络文件系统（NFS/SMB/CIFS）。
```

---

### 补充的错误处理

#### [ERR-1] NDJSON 损坏行处理（影响 T03）

**T03 补充约束**:
```
- iter_events 遇到 JSON 解析失败的行时：
  1. 记录 tracing::warn! 日志（包含行号和原始内容前 200 字符）
  2. 跳过该行，继续读取下一行
  3. 不 panic，不中止迭代
- read_last_lines 遇到损坏行时同样跳过
```

#### [ERR-2] Socket 缓冲区溢出保护（影响 T06）

**T06 补充约束**:
```
- 单个 JSON 请求最大 1MB（超过则断开连接并返回 error）
- read_request 使用带上限的 read_line：
  let mut line = String::new();
  let n = reader.take(1_048_576).read_line(&mut line).await?;
  if n >= 1_048_576 { return Err("request_too_large") }
```

#### [ERR-3] Daemon/Plugin 版本不匹配（影响 T17）

**T07 补充**: `ping` 响应包含版本信息：
```json
{ "v": 1, "ok": true, "result": { "version": "0.1.0", "pid": 12345, "has_unread": false } }
```

**T17 补充约束**:
```
- Plugin 在 ensureDaemon 成功后检查 ping 返回的 version
- 主版本号不匹配时：停止旧 Daemon → 重新下载/启动新版本
- 次版本号不匹配时：仅记录警告日志
```

---

### 补充的 PBT 属性

| 任务 | 属性 | 类别 | 伪造策略 |
|------|------|------|---------|
| T02 | Event.seq 严格单调递增 | monotonicity | 并发追加 1000 事件，验证 seq 序列无跳跃无重复 |
| T04 | Blob 完整性（含特殊字符） | round-trip | 生成含 NULL/换行/非 UTF-8 的 Body → 溢出 → 读回 → 逐字节比较 |
| T12 | 游标并发更新单调性 | monotonicity | 并发将游标设为 seq=10 和 seq=20，验证最终游标 >= 20 |
| T13 | 投递队列有界性 | bounds | Actor 离线时发送 2000 条消息，验证队列 <= 1000 且最旧丢弃 |
| T06 | 恶意请求不导致 crash | invariant | 发送随机二进制数据/超大 JSON/畸形 JSON，验证 Daemon 存活 |

---

## 审计状态

| 检查项 | 状态 |
|--------|------|
| Gemini 集成分析 | 已完成 |
| Codex 后端分析 | 失败（Codex CLI 异常退出，由 Claude 自行补充） |
| Blocker 级歧义 | 1 个，已消除 (AMB-1) |
| Warning 级歧义 | 2 个，已消除 (AMB-2, AMB-3) |
| 竞态条件 | 2 个，已消除 (RACE-1, RACE-2) |
| 隐含假设 | 3 个，已显式化 (ASSUM-1, ASSUM-2, ASSUM-3) |
| 错误处理补充 | 3 个 (ERR-1, ERR-2, ERR-3) |
| PBT 属性补充 | 5 个 |
| **零残留歧义** | **是** |

---

*Phase 1 任务清单完成：2026-02-28*
*歧义消除审计完成：2026-02-28*
*下一步：按阶段 A → H 顺序逐个实施*
