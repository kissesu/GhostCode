# Team Plan: Phase 1.5 连接层补丁

## 概述

补全 Phase 1 遗留的端到端连接断层：让 Daemon 真正能启动，让 Plugin 真正能调通 MCP 工具。
三个补丁任务：main.rs 启动代码、settings.json 修复、端到端冒烟测试。

## Codex 分析摘要

Codex CLI 不可用（exit 1），由 Claude 自行分析。

后端分析结论：
- main.rs 的所有依赖模块已就绪（DaemonPaths, lock, process, server, AppState）
- 启动流程清晰：clap 参数解析 → 路径初始化 → 单实例锁 → 清理残留 → AppState → 写 addr.json/pid → serve_forever
- 信号处理需用 tokio::signal::ctrl_c() 触发 AppState.trigger_shutdown()
- 风险低：所有子模块已有独立测试（10 个测试文件）

## Gemini 分析摘要

Gemini 分析了 settings.json 配置问题：
1. `skills/` 目录不存在，应移除该字段
2. `shell: false` 会阻止 Plugin 启动 Daemon 子进程，需改为允许
3. `filesystem.write: false` 会导致 Ledger 账本无法写入，需改为 true
4. 建议添加显式 `mcpServers` 配置声明

## 技术方案

### Task 1: main.rs 完整启动代码

**设计说明**:

main.rs 需要将已有的各模块串联起来，形成完整的 Daemon 启动流程。
所有依赖模块已实现并测试通过，main.rs 只做编排。

**启动流程**:
```
1. clap 解析 CLI 参数（--base-dir 可选，默认 ~/.ghostcode）
2. tracing 日志初始化
3. DaemonPaths::new(base_dir) 路径初始化
4. 创建 daemon_dir 和 groups_dir 目录
5. try_acquire_singleton_lock() 单实例锁
6. cleanup_stale_files() 清理残留
7. AppState::new(groups_dir)
8. 写入 addr.json（AddrDescriptor）
9. 写入 PID 文件
10. 注册 Ctrl+C 信号处理 → trigger_shutdown()
11. serve_forever(config, state)
12. 退出时清理 addr.json + pid + socket
```

**Cargo.toml 注意**：
- `clap` 已在 dependencies 中（workspace 级别）
- `tracing` + `tracing-subscriber` 已在 dependencies 中
- 无需新增依赖

### Task 2: settings.json 修复

**修改内容**:
- 移除 `skills` 字段（目录不存在）
- `filesystem.write` 改为 `true`（Ledger 需要写入）
- `shell` 改为 `true`（启动 Daemon 子进程）
- 注意：Claude Code Plugin 的 settings.json 格式以实际 Claude Code 文档为准

### Task 3: 端到端冒烟测试

**测试脚本**：验证 Daemon 二进制可以启动、响应 ping、优雅关闭。
使用 Rust 集成测试，直接调用 `serve_forever` 函数（与 T19 集成测试同模式）。

**测试场景**:
1. Daemon 启动后 addr.json 存在
2. 通过 Unix Socket 发送 ping → 收到 pong
3. 发送 shutdown → Daemon 退出
4. 退出后 socket 文件被清理
5. 单实例锁阻止第二个实例启动

---

## 子任务列表

### Task 1: 实现 main.rs 完整启动代码

- **类型**: 后端（Rust）
- **文件范围**: `crates/ghostcode-daemon/src/main.rs`（修改）
- **依赖**: 无
- **TDD 流程**: Red → Green → Refactor
- **实施步骤**:

  **Red 阶段（测试先行）**:
  1. 创建测试文件 `crates/ghostcode-daemon/tests/main_test.rs`
  2. 编写以下测试用例：
     - `test_daemon_starts_and_creates_addr_json`: 启动 Daemon → 验证 addr.json 存在
     - `test_daemon_ping_pong`: 启动 → 连接 socket → 发送 ping → 收到 pong
     - `test_daemon_shutdown_cleanup`: 启动 → shutdown → socket/addr.json 被清理
     - `test_singleton_lock_prevents_double_start`: 占用锁 → 第二个 Daemon 启动失败
  3. 运行 `cargo test --test main_test` 确认测试失败（Red）

  **Green 阶段（最小实现）**:
  4. 修改 `main.rs`，实现完整启动逻辑：

  ```rust
  use std::path::PathBuf;
  use std::sync::Arc;

  use clap::Parser;
  use tracing_subscriber;

  use ghostcode_daemon::paths::DaemonPaths;
  use ghostcode_daemon::lock::try_acquire_singleton_lock;
  use ghostcode_daemon::process::{cleanup_stale_files, write_addr_descriptor, write_pid_file};
  use ghostcode_daemon::server::{AppState, DaemonConfig, serve_forever};
  use ghostcode_types::addr::AddrDescriptor;

  /// GhostCode Daemon 命令行参数
  #[derive(Parser, Debug)]
  #[command(name = "ghostcoded", about = "GhostCode 常驻守护进程")]
  struct Args {
      /// 基础目录路径（默认 ~/.ghostcode）
      #[arg(long, default_value_t = default_base_dir())]
      base_dir: String,
  }

  fn default_base_dir() -> String {
      let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
      format!("{}/.ghostcode", home)
  }

  #[tokio::main]
  async fn main() -> anyhow::Result<()> {
      // 1. 日志初始化
      tracing_subscriber::fmt::init();

      // 2. 解析 CLI 参数
      let args = Args::parse();
      let base_dir = PathBuf::from(&args.base_dir);

      // 3. 路径初始化
      let paths = DaemonPaths::new(&base_dir);

      // 4. 创建必要目录
      std::fs::create_dir_all(&paths.daemon_dir)?;
      let groups_dir = base_dir.join("groups");
      std::fs::create_dir_all(&groups_dir)?;

      // 5. 单实例锁
      let _lock = try_acquire_singleton_lock(&paths.lock)?;
      tracing::info!("单实例锁获取成功");

      // 6. 清理残留文件
      cleanup_stale_files(&paths.daemon_dir)?;

      // 7. AppState 初始化
      let state = Arc::new(AppState::new(groups_dir));

      // 8. 写入 addr.json
      let descriptor = AddrDescriptor {
          v: 1,
          transport: "unix".to_string(),
          path: paths.sock.to_string_lossy().to_string(),
          pid: std::process::id(),
          version: env!("CARGO_PKG_VERSION").to_string(),
          ts: chrono::Utc::now().to_rfc3339(),
      };
      write_addr_descriptor(&paths.addr, &descriptor)?;

      // 9. 写入 PID 文件
      write_pid_file(&paths.pid, std::process::id())?;

      tracing::info!(
          socket = %paths.sock.display(),
          pid = std::process::id(),
          "GhostCode Daemon 启动"
      );

      // 10. 注册 Ctrl+C 信号处理
      let state_for_signal = Arc::clone(&state);
      tokio::spawn(async move {
          let _ = tokio::signal::ctrl_c().await;
          tracing::info!("收到 Ctrl+C，准备关闭");
          state_for_signal.trigger_shutdown();
      });

      // 11. 启动服务
      let config = DaemonConfig {
          socket_path: paths.sock.clone(),
      };
      serve_forever(config, state).await?;

      // 12. 退出清理
      let _ = std::fs::remove_file(&paths.addr);
      let _ = std::fs::remove_file(&paths.pid);
      tracing::info!("GhostCode Daemon 已关闭");

      Ok(())
  }
  ```

  5. 运行 `cargo test --test main_test` 确认测试通过（Green）
  6. 运行 `cargo build` 确认编译通过
  7. 运行 `cargo clippy -- -D warnings` 确认零警告

- **验收标准**:
  - `cargo build` 零错误
  - `cargo clippy -- -D warnings` 零警告
  - `cargo test --test main_test` 所有测试通过
  - 手动执行 `./target/debug/ghostcode-daemon --base-dir /tmp/ghostcode-test` 可启动并响应 ping

---

### Task 2: 修复 settings.json

- **类型**: 配置修复
- **文件范围**: `src/plugin/.claude/settings.json`（修改）
- **依赖**: 无
- **实施步骤**:
  1. 读取现有 `src/plugin/.claude/settings.json`
  2. 移除 `"skills": "./skills/"` 字段
  3. 将 `"filesystem": { "read": true, "write": false }` 改为 `"filesystem": { "read": true, "write": true }`
  4. 将 `"shell": false` 改为 `"shell": true`
  5. 验证 JSON 语法正确

- **验收标准**: JSON 语法正确，无悬挂引用。

---

### Task 3: 端到端冒烟测试

- **类型**: 测试（Rust 集成测试）
- **文件范围**: `crates/ghostcode-daemon/tests/main_test.rs`（新建）
- **依赖**: Task 1
- **TDD 说明**: 此任务是 Task 1 TDD Red 阶段的测试文件。Task 1 的 Builder 需先创建此测试文件再写实现。但由于 main.rs 是二进制入口无法直接 import，测试改为通过 `serve_forever` 函数级测试。
- **实施步骤**:
  1. 创建 `crates/ghostcode-daemon/tests/main_test.rs`
  2. 编写以下测试用例：

  ```rust
  //! 端到端冒烟测试：验证 Daemon 启动→响应→关闭的完整生命周期
  use std::sync::Arc;
  use std::time::Duration;
  use tempfile::TempDir;
  use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
  use tokio::net::UnixStream;

  use ghostcode_daemon::paths::DaemonPaths;
  use ghostcode_daemon::lock::try_acquire_singleton_lock;
  use ghostcode_daemon::process::{cleanup_stale_files, write_addr_descriptor, write_pid_file};
  use ghostcode_daemon::server::{AppState, DaemonConfig, serve_forever};
  use ghostcode_types::addr::AddrDescriptor;

  /// 辅助函数：启动 Daemon 并返回 state + socket_path
  async fn start_test_daemon(base_dir: &std::path::Path) -> (Arc<AppState>, std::path::PathBuf) {
      let paths = DaemonPaths::new(base_dir);
      std::fs::create_dir_all(&paths.daemon_dir).unwrap();
      let groups_dir = base_dir.join("groups");
      std::fs::create_dir_all(&groups_dir).unwrap();

      cleanup_stale_files(&paths.daemon_dir).unwrap();

      let state = Arc::new(AppState::new(groups_dir));
      let descriptor = AddrDescriptor {
          v: 1,
          transport: "unix".to_string(),
          path: paths.sock.to_string_lossy().to_string(),
          pid: std::process::id(),
          version: "0.1.0-test".to_string(),
          ts: chrono::Utc::now().to_rfc3339(),
      };
      write_addr_descriptor(&paths.addr, &descriptor).unwrap();

      let config = DaemonConfig { socket_path: paths.sock.clone() };
      let state_clone = Arc::clone(&state);
      tokio::spawn(async move {
          let _ = serve_forever(config, state_clone).await;
      });

      // 等待 socket 就绪
      tokio::time::sleep(Duration::from_millis(100)).await;

      (state, paths.sock)
  }

  #[tokio::test]
  async fn test_daemon_ping_pong() {
      let dir = TempDir::new().unwrap();
      let (state, sock_path) = start_test_daemon(dir.path()).await;

      let stream = UnixStream::connect(&sock_path).await.unwrap();
      let (read, mut write) = stream.into_split();
      let mut reader = BufReader::new(read);

      // 发送 ping
      let req = r#"{"v":1,"op":"ping","args":{}}"#;
      write.write_all(req.as_bytes()).await.unwrap();
      write.write_all(b"\n").await.unwrap();

      // 读取响应
      let mut line = String::new();
      reader.read_line(&mut line).await.unwrap();
      let resp: serde_json::Value = serde_json::from_str(&line).unwrap();
      assert_eq!(resp["ok"], true);
      assert_eq!(resp["result"]["pong"], true);

      state.trigger_shutdown();
      tokio::time::sleep(Duration::from_millis(200)).await;
  }

  #[tokio::test]
  async fn test_daemon_shutdown_cleanup() {
      let dir = TempDir::new().unwrap();
      let (state, sock_path) = start_test_daemon(dir.path()).await;

      assert!(sock_path.exists(), "socket 文件应存在");

      state.trigger_shutdown();
      tokio::time::sleep(Duration::from_secs(6)).await;

      assert!(!sock_path.exists(), "shutdown 后 socket 应被清理");
  }

  #[tokio::test]
  async fn test_singleton_lock() {
      let dir = TempDir::new().unwrap();
      let paths = DaemonPaths::new(dir.path());
      std::fs::create_dir_all(&paths.daemon_dir).unwrap();

      let _lock1 = try_acquire_singleton_lock(&paths.lock).unwrap();
      let lock2_result = try_acquire_singleton_lock(&paths.lock);
      assert!(lock2_result.is_err(), "第二个锁应该失败");
  }

  #[tokio::test]
  async fn test_addr_json_created() {
      let dir = TempDir::new().unwrap();
      let paths = DaemonPaths::new(dir.path());
      std::fs::create_dir_all(&paths.daemon_dir).unwrap();

      let descriptor = AddrDescriptor {
          v: 1,
          transport: "unix".to_string(),
          path: "/tmp/test.sock".to_string(),
          pid: 12345,
          version: "0.1.0".to_string(),
          ts: "2026-03-02T00:00:00Z".to_string(),
      };
      write_addr_descriptor(&paths.addr, &descriptor).unwrap();

      assert!(paths.addr.exists(), "addr.json 应被创建");
      let content = std::fs::read_to_string(&paths.addr).unwrap();
      let loaded: AddrDescriptor = serde_json::from_str(&content).unwrap();
      assert_eq!(loaded.pid, 12345);
  }
  ```

  3. 运行 `cargo test --test main_test` 验证所有测试通过

- **验收标准**: 4 个测试全部通过

---

## 文件冲突检查

| 文件路径 | 状态 | 冲突 |
|---------|------|------|
| `crates/ghostcode-daemon/src/main.rs` | 修改 | 无（Task 1 独占） |
| `src/plugin/.claude/settings.json` | 修改 | 无（Task 2 独占） |
| `crates/ghostcode-daemon/tests/main_test.rs` | 新建 | 无（Task 3 独占） |

✅ 无冲突，三个文件范围完全隔离。

---

## TDD 强制执行规范

```
Red    → Task 3 先写测试文件 main_test.rs（编译通过但断言失败）
Green  → Task 1 实现 main.rs 启动代码（让测试通过）
Config → Task 2 修复 settings.json（独立，无测试依赖）
```

注意：Task 1 和 Task 3 的 TDD 流程：
- Builder 先创建 main_test.rs（Red）
- 再修改 main.rs（Green）
- 由于二者是同一个 Builder 负责（文件有依赖），合并为一个 Builder 执行

---

## 并行分组

```
Layer 1（并行）:
  Task 1+3（合并）— main.rs 实现 + main_test.rs 测试（TDD Red→Green）
  Task 2          — settings.json 修复（独立，无依赖）

验证: cargo build 零警告 + cargo test --test main_test 全通过
```

---

## Builder 配置

```yaml
builder_count: 2
model: sonnet

builder-main:
  tasks: [Task 1, Task 3]
  files: [main.rs, main_test.rs]
  tdd: true

builder-config:
  tasks: [Task 2]
  files: [settings.json]
  tdd: false
```
