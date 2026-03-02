# Team Plan: Phase 1.5 Daemon 端到端启动链路补丁

## 概述
补齐 Phase 1 诊断发现的两个端到端缺口：P1（main.rs 启动逻辑）+ P2（Plugin Hook 自动启动 Daemon），使 Daemon 可以真正启动运行并被 Plugin 自动管理。

## 分析摘要
基于对代码库的完整分析（Codex/Gemini 因外部原因不可用，由 Claude 独立完成）：

### Rust 端（P1 - main.rs）
- **已完备模块**: lock.rs、paths.rs、process.rs、server.rs、dispatch.rs、所有 messaging 模块
- **唯一缺口**: main.rs 仅有 `println!("GhostCode Daemon starting...")`，约需 100 行启动逻辑
- **依赖已就绪**: Cargo.toml 包含 clap、tokio、tracing、tracing-subscriber 等全部依赖
- **启动流程**: 获取锁 → 初始化路径 → 清理残留 → 创建 AppState → 写入 addr.json/PID → 信号处理 → serve_forever

### TypeScript 端（P2 - Hook 连接）
- **已完备模块**: daemon.ts（ensureDaemon/stopDaemon/startHeartbeat）、hooks/index.ts（registerHook/getHooks 基础设施）
- **缺口**: hooks/index.ts 无实际处理器，Plugin 初始化时不注册 Hook
- **需要**: PreToolUse Hook 处理器 + Plugin 初始化注册逻辑 + Stop Hook 清理

## 技术方案

### P1: main.rs 启动逻辑
```
main() → tokio runtime →
  1. clap 解析 --groups-dir（默认 ~/.ghostcode/groups）
  2. DaemonPaths::new(base_dir) 获取路径
  3. try_acquire_singleton_lock(paths.lock) 获取排他锁
  4. cleanup_stale_files(paths.daemon_dir) 清理残留
  5. tracing_subscriber::init() 初始化日志
  6. AppState::new(groups_dir) 创建状态
  7. write_addr_descriptor + write_pid_file 写入连接信息
  8. tokio::signal 监听 SIGTERM/SIGINT → trigger_shutdown
  9. serve_forever(config, state) 启动服务
  10. 退出时清理 addr.json/PID 文件
```

### P2: Plugin Hook 连接
```
hooks/handlers.ts:
  - preToolUseHandler(): 调用 ensureDaemon()
  - stopHandler(): 停止心跳 + stopDaemon()

hooks/index.ts 增强:
  - initializeHooks(): 注册所有 Hook 处理器

index.ts:
  - Plugin activate() 中调用 initializeHooks()
```

## 子任务列表

### Task 1: main.rs 启动逻辑测试 (TDD Red)
- **类型**: 后端 (Rust)
- **文件范围**: `crates/ghostcode-daemon/tests/main_startup_test.rs`
- **依赖**: 无
- **实施步骤**:
  1. 创建测试文件 `tests/main_startup_test.rs`
  2. 编写以下测试用例：
     - `test_startup_creates_addr_json`: 启动后 addr.json 存在且内容合法
     - `test_startup_creates_pid_file`: 启动后 PID 文件存在且内容为有效数字
     - `test_startup_acquires_lock`: 启动后锁文件被占用，第二次启动返回 AlreadyRunning
     - `test_startup_cleans_stale_files`: 预先放置残留文件 → 启动时被清理
     - `test_startup_responds_to_ping`: 启动后通过 socket 发送 ping 收到 pong
     - `test_shutdown_via_signal`: 启动后发送 shutdown 请求 → 进程优雅退出
     - `test_shutdown_cleans_socket_and_addr`: shutdown 后 socket 和 addr.json 被清理
  3. 由于 main.rs 是二进制入口，测试采用将核心启动逻辑抽取到 lib.rs 的可测试函数方式
  4. 具体做法：测试调用一个新的 `pub async fn run_daemon(config: StartupConfig) -> Result<()>` 函数
  5. 运行 `cargo test --test main_startup_test` 确认所有测试编译通过但断言失败（Red）
- **验收标准**: 所有测试编译通过，运行时断言失败（Red 阶段）

### Task 2: main.rs 启动逻辑实现 (TDD Green)
- **类型**: 后端 (Rust)
- **文件范围**:
  - `crates/ghostcode-daemon/src/startup.rs`（新建）
  - `crates/ghostcode-daemon/src/main.rs`（修改）
  - `crates/ghostcode-daemon/src/lib.rs`（添加 `pub mod startup;`）
- **依赖**: Task 1
- **实施步骤**:
  1. 创建 `src/startup.rs`，实现 `run_daemon` 函数：
     ```rust
     pub struct StartupConfig {
         pub base_dir: PathBuf,
         pub groups_dir: PathBuf,
     }

     pub async fn run_daemon(config: StartupConfig) -> Result<()> {
         // 1. 初始化路径
         let paths = DaemonPaths::new(&config.base_dir);
         // 2. 获取单实例锁
         let _lock = try_acquire_singleton_lock(&paths.lock)?;
         // 3. 清理残留文件
         cleanup_stale_files(&paths.daemon_dir)?;
         // 4. 创建 AppState
         let state = Arc::new(AppState::new(config.groups_dir));
         // 5. 写入 addr.json 和 PID
         let descriptor = AddrDescriptor::new(
             paths.sock.to_string_lossy(),
             std::process::id(),
             env!("CARGO_PKG_VERSION")
         );
         write_addr_descriptor(&paths.addr, &descriptor)?;
         write_pid_file(&paths.pid, std::process::id())?;
         // 6. 设置信号处理
         let shutdown_state = Arc::clone(&state);
         tokio::spawn(async move {
             let _ = tokio::signal::ctrl_c().await;
             shutdown_state.trigger_shutdown();
         });
         // 7. 启动服务
         let daemon_config = DaemonConfig { socket_path: paths.sock.clone() };
         serve_forever(daemon_config, state).await?;
         // 8. 清理
         let _ = std::fs::remove_file(&paths.addr);
         let _ = std::fs::remove_file(&paths.pid);
         Ok(())
     }
     ```
  2. 修改 `src/main.rs`，使用 clap 解析参数并调用 `run_daemon`：
     ```rust
     #[derive(Parser)]
     struct Cli {
         #[arg(long, default_value = "~/.ghostcode")]
         base_dir: PathBuf,
         #[arg(long)]
         groups_dir: Option<PathBuf>,
     }

     #[tokio::main]
     async fn main() -> Result<()> {
         // 初始化 tracing
         tracing_subscriber::fmt::init();
         // 解析参数
         let cli = Cli::parse();
         let base_dir = expand_tilde(&cli.base_dir);
         let groups_dir = cli.groups_dir.unwrap_or_else(|| base_dir.join("groups"));
         // 运行
         run_daemon(StartupConfig { base_dir, groups_dir }).await
     }
     ```
  3. 在 `lib.rs` 中添加 `pub mod startup;`
  4. 运行 `cargo test --test main_startup_test` 确认所有测试通过（Green）
  5. 运行 `cargo build` 确认编译零错误
- **验收标准**: 所有 main_startup_test 测试通过 + cargo build 零错误

### Task 3: Plugin Hook 处理器测试 (TDD Red)
- **类型**: 前端 (TypeScript)
- **文件范围**: `src/plugin/src/hooks/handlers.test.ts`（新建）
- **依赖**: 无
- **实施步骤**:
  1. 创建测试文件 `src/plugin/src/hooks/handlers.test.ts`
  2. 编写以下测试用例（使用 vitest mock）：
     - `preToolUseHandler calls ensureDaemon`: 调用 preToolUseHandler 后 ensureDaemon 被调用
     - `preToolUseHandler caches result`: 第二次调用不再触发 ensureDaemon（复用 Promise）
     - `preToolUseHandler handles error gracefully`: ensureDaemon 抛错时不阻断工具调用
     - `stopHandler calls stopDaemon`: 调用 stopHandler 后 stopDaemon 被调用
     - `stopHandler stops heartbeat if running`: 心跳运行时 stopHandler 调用停止函数
     - `initializeHooks registers all handlers`: 初始化后各事件类型都有处理器
  3. 运行 `pnpm --filter ghostcode-plugin test` 确认测试编译通过但断言失败（Red）
- **验收标准**: 所有测试编译通过，运行时断言失败（Red 阶段）

### Task 4: Plugin Hook 处理器实现 (TDD Green)
- **类型**: 前端 (TypeScript)
- **文件范围**:
  - `src/plugin/src/hooks/handlers.ts`（新建）
  - `src/plugin/src/hooks/index.ts`（修改 - 添加 initializeHooks）
  - `src/plugin/src/index.ts`（修改 - 添加 initializeHooks 调用）
- **依赖**: Task 3
- **实施步骤**:
  1. 创建 `src/plugin/src/hooks/handlers.ts`：
     ```typescript
     import { ensureDaemon, stopDaemon, startHeartbeat } from "../daemon";
     import type { AddrDescriptor } from "../daemon";

     let daemonPromise: Promise<AddrDescriptor> | null = null;
     let stopHeartbeatFn: (() => void) | null = null;

     // PreToolUse Hook：确保 Daemon 运行
     export async function preToolUseHandler(): Promise<void> {
       if (daemonPromise === null) {
         daemonPromise = ensureDaemon().then((addr) => {
           if (stopHeartbeatFn === null) {
             stopHeartbeatFn = startHeartbeat(addr);
           }
           return addr;
         });
       }
       await daemonPromise;
     }

     // Stop Hook：清理资源
     export async function stopHandler(): Promise<void> {
       if (stopHeartbeatFn !== null) {
         stopHeartbeatFn();
         stopHeartbeatFn = null;
       }
       await stopDaemon();
       daemonPromise = null;
     }
     ```
  2. 修改 `src/plugin/src/hooks/index.ts`，添加 `initializeHooks` 函数：
     ```typescript
     import { preToolUseHandler, stopHandler } from "./handlers";

     export function initializeHooks(): void {
       registerHook("PreToolUse", preToolUseHandler);
       registerHook("Stop", stopHandler);
     }
     ```
  3. 修改 `src/plugin/src/index.ts`，在 Plugin activate 中调用 `initializeHooks()`
  4. 运行 `pnpm --filter ghostcode-plugin test` 确认所有测试通过（Green）
  5. 运行 `pnpm --filter ghostcode-plugin build` 确认构建成功
- **验收标准**: 所有 handlers.test.ts 测试通过 + build 零错误

### Task 5: 端到端验证
- **类型**: 后端 + 前端
- **文件范围**: 无新文件（只运行测试和构建）
- **依赖**: Task 2, Task 4
- **实施步骤**:
  1. 运行 `cargo test --workspace` 确认所有 Rust 测试通过
  2. 运行 `cargo build --release` 确认 Release 构建成功
  3. 运行 `pnpm --filter ghostcode-plugin test` 确认所有 TS 测试通过
  4. 运行 `pnpm --filter ghostcode-plugin build` 确认 TS 构建成功
  5. 手动验证：`cargo run -- --base-dir /tmp/ghostcode-test` 启动 Daemon，ping 测试
- **验收标准**: 全部测试通过 + 全部构建零错误

## 文件冲突检查
✅ 无冲突 - 各 Task 文件范围完全隔离：
- Task 1: `tests/main_startup_test.rs`（新建，独占）
- Task 2: `src/startup.rs`（新建）+ `src/main.rs`（修改）+ `src/lib.rs`（仅添加一行 mod）
- Task 3: `src/plugin/src/hooks/handlers.test.ts`（新建，独占）
- Task 4: `src/plugin/src/hooks/handlers.ts`（新建）+ `hooks/index.ts`（修改）+ `index.ts`（修改）
- Task 5: 无文件修改，只运行验证

## 并行分组
- **Layer 1 (并行)**: Task 1（Rust 测试）, Task 3（TS 测试）— TDD Red 阶段
- **Layer 2 (依赖 Layer 1)**: Task 2（Rust 实现，依赖 Task 1）, Task 4（TS 实现，依赖 Task 3）— TDD Green 阶段
- **Layer 3 (依赖 Layer 2)**: Task 5（端到端验证）
