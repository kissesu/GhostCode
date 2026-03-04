# GhostCode Phase 2 开发任务清单

> 目标：多模型路由引擎 + 代码主权控制 + DAG 并行执行
> 日期：2026-03-02
> 状态：规划完成，待实施
> 前置：Phase 1 (T01-T20) 全部完成

---

## 成功判据

| # | 判据 | 验证方式 |
|---|------|---------|
| OK-1 | 任务自动路由到正确后端（前端→Gemini, 后端→Codex） | 集成测试：提交 CSS 任务→Gemini, API 任务→Codex |
| OK-2 | 外部模型（Codex/Gemini）无法直接写入文件系统 | 测试：非 Claude 后端尝试写入→被拒绝 |
| OK-3 | DAG 并行执行引擎正确调度有依赖的任务集 | 测试：5 个任务含依赖→层内并行 + 层间串行 |
| OK-4 | SESSION_ID 复用保留上下文 | 测试：execute→获取 session_id→resume→上下文连续 |
| OK-5 | JSON Stream 统一解析三种后端输出 | 测试：混合 Codex/Claude/Gemini 格式→统一事件流 |
| OK-6 | TS Plugin 通过 IPC 提交路由任务并接收流式结果 | 端到端：Plugin→IPC→Daemon→Router→结果流 |

---

## 技术选型（Phase 2 新增）

| 类别 | 选择 | 用途 |
|------|------|------|
| 正则 | `regex` | ROLE_FILE 注入的正则匹配 |
| 信号量 | `tokio::sync::Semaphore` | DAG 并行执行的并发限流 |
| 子进程 | `tokio::process::Command` | 异步子进程管理（替代 Go 的 exec.Command） |
| 取消令牌 | `tokio_util::sync::CancellationToken` | 任务取消传播 |

沿用 Phase 1 的：tokio, serde, serde_json, uuid, chrono, tracing, thiserror, anyhow, proptest, fs2

---

## 新增 Crate 结构

```
crates/
  ghostcode-router/         ← 新增：多模型路由引擎
    Cargo.toml
    src/
      lib.rs                ← 模块导出
      backend.rs            ← Backend trait + 三后端实现
      stream.rs             ← JSON Stream 统一解析器
      session.rs            ← SESSION_ID 管理
      rolefile.rs           ← ROLE_FILE 注入
      process.rs            ← 异步子进程管理
      dag.rs                ← DAG 拓扑排序
      executor.rs           ← 并行执行引擎
      task_format.rs        ← ---TASK---/---CONTENT--- 解析
      sovereignty.rs        ← 代码主权控制
    tests/
      backend_test.rs
      stream_test.rs
      session_test.rs
      rolefile_test.rs
      process_test.rs
      dag_test.rs
      executor_test.rs
      task_format_test.rs
      sovereignty_test.rs
```

---

## 参考项目源码对照

| Phase 2 模块 | 参考源码 | 关键文件 |
|-------------|---------|---------|
| Backend trait | ccg-workflow | codeagent-wrapper/backend.go:25-145 |
| JSON Stream 解析 | ccg-workflow | codeagent-wrapper/parser.go:72-90 |
| SESSION_ID 管理 | ccg-workflow | codeagent-wrapper/parser.go:160-163, main.go:483-485 |
| ROLE_FILE 注入 | ccg-workflow | codeagent-wrapper/utils.go:75-117 |
| 子进程管理 | ccg-workflow | codeagent-wrapper/executor.go:810-1280 |
| DAG 拓扑排序 | ccg-workflow | codeagent-wrapper/executor.go:287-351 |
| 并行执行引擎 | ccg-workflow | codeagent-wrapper/executor.go:353-515 |
| 任务格式解析 | ccg-workflow | codeagent-wrapper/config.go:113-185 |
| 代码主权 | ccg-workflow | templates/commands/execute.md:208-237, README.md:221 |
| 路由配置 | ccg-workflow | src/types/index.ts (ModelRouting) |

---

## 与 Phase 1 集成点

| Phase 1 模块 | 集成方式 | 影响程度 |
|-------------|---------|---------|
| ghostcode-types/actor.rs | RuntimeKind 已有 Claude/Codex/Gemini/Custom | 无需修改 |
| ghostcode-daemon/dispatch.rs | 新增 5 个 op handler（追加代码） | 小改 |
| ghostcode-daemon/lifecycle.rs | start_actor 时携带 routing 元数据 | 小改 |
| ghostcode-mcp/tools/ | 新增 3 个 MCP 工具（追加文件） | 追加 |
| src/plugin/hooks/ | 新增路由决策 Hook（追加文件） | 追加 |
| src/plugin/ipc.ts | 支持 streaming response（增强） | 小改 |

---

## 任务详细定义

### 阶段 E：路由核心 (ghostcode-router crate)

#### T21 - Backend Trait + CLI 参数构建器

**前置依赖**: T01 (Cargo Workspace)
**产出**: `crates/ghostcode-router/src/backend.rs`
**参考**: `ccg-workflow/codeagent-wrapper/backend.go:25-145`

**需要定义的类型**:

```rust
/// 后端任务配置
struct TaskConfig {
    workdir: PathBuf,          // 工作目录
    mode: TaskMode,            // New 或 Resume
    session_id: Option<String>, // 仅 Resume 模式
    model: Option<String>,     // 仅 Gemini 需要
    timeout: Duration,         // 执行超时
}

/// 任务模式
enum TaskMode {
    New,     // 新建会话
    Resume,  // 恢复已有会话
}

/// 后端 trait（策略模式）
trait Backend: Send + Sync {
    /// 后端名称（"codex"/"claude"/"gemini"）
    fn name(&self) -> &str;
    /// CLI 可执行命令名
    fn command(&self) -> &str;
    /// 根据配置构建命令行参数
    fn build_args(&self, config: &TaskConfig) -> Vec<String>;
}
```

**三个具体实现的参数映射**:

| 后端 | 命令 | 新会话必选参数 | Resume 参数 |
|------|------|-------------|------------|
| Codex | `codex` | `e --dangerously-bypass-approvals-and-sandbox --skip-git-repo-check -C <workdir> --json` | `resume <session_id>`（无 -C） |
| Claude | `claude` | `-p --dangerously-skip-permissions --setting-sources "" --output-format stream-json --verbose` | `-r <session_id>` |
| Gemini | `gemini` | `-m <model> -o stream-json -y -p` | `-r <session_id>` |

**约束**:
- Backend trait 使用 Send + Sync bound（支持跨 tokio task 共享）
- Codex resume 模式不传 `-C`（参考: backend.go:25-27）
- Claude 必须包含 `--setting-sources ""`（防止递归调用 CLAUDE.md/skills）
- Gemini 默认 model 从环境变量 `GEMINI_MODEL` 读取，fallback 到 "gemini-2.5-pro"

**PBT 属性**:
- 参数安全性：随机 TaskConfig → build_args() 输出始终包含安全标志（--dangerously-bypass 等）
- Resume 完整性：所有后端的 resume 模式参数包含 session_id
- New 纯净性：new 模式不包含 session_id 相关参数
- 参数非空：build_args() 结果长度 > 0

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-router/tests/backend_test.rs
   - #[test] fn codex_new_args_contain_required_flags()
     → 验证包含 --dangerously-bypass-approvals-and-sandbox
   - #[test] fn codex_new_args_contain_workdir()
     → 验证包含 -C <workdir>
   - #[test] fn codex_resume_no_workdir()
     → 验证 resume 模式不包含 -C
   - #[test] fn claude_new_args_contain_setting_sources_empty()
     → 验证包含 --setting-sources ""
   - #[test] fn claude_resume_contains_r_flag()
     → 验证包含 -r <session_id>
   - #[test] fn gemini_new_args_contain_model()
     → 验证包含 -m <model>
   - #[test] fn gemini_resume_contains_r_flag()
     → 验证包含 -r <session_id>
   - proptest! fn all_backends_args_nonEmpty(config in arb_task_config())
     → 三个后端的 build_args() 结果均非空
   - proptest! fn resume_always_contains_session_id(
         session_id in "[a-z0-9]{8,32}"
     )
     → 三个后端的 resume 模式参数均包含 session_id
   → cargo test → 全部编译失败（Backend trait 不存在）

🟢 Green: 实现 Backend trait + CodexBackend + ClaudeBackend + GeminiBackend
   → cargo test → 全部通过

🔵 Refactor: 提取公共安全标志到常量
   → cargo test + clippy → 零警告
```

---

#### T22 - JSON Stream 统一解析器

**前置依赖**: T21
**产出**: `crates/ghostcode-router/src/stream.rs`
**参考**: `ccg-workflow/codeagent-wrapper/parser.go:72-163`

**需要定义的类型**:

```rust
/// 统一事件类型（抹平三种后端差异）
enum StreamEventKind {
    Init,          // 会话初始化
    Progress,      // 执行进度
    AgentMessage,  // Agent 输出（核心内容）
    Complete,      // 执行完成
    Error,         // 错误
}

/// 统一事件
struct StreamEvent {
    kind: StreamEventKind,
    content: Option<String>,    // 消息内容
    session_id: Option<String>, // 会话 ID（首次出现时提取）
    raw: serde_json::Value,     // 原始 JSON（调试用）
}

/// 流解析器
struct StreamParser {
    locked_session_id: Option<String>, // 一旦锁定不再覆盖
}
```

**后端检测逻辑**（参考: parser.go:72-90）:
- Codex 事件：有 `thread_id` 或 `item` 字段
- Claude 事件：有 `subtype` 或 `result` 字段
- Gemini 事件：有 `role` 或 `delta` 或 `status` 字段

**SESSION_ID 提取规则**（参考: parser.go:160-163）:
| 后端 | 事件类型 | 字段名 |
|------|---------|--------|
| Codex | `thread.started` | `thread_id` |
| Claude | `result` | `session_id` |
| Gemini | `init` | `session_id` |

**约束**:
- 异步 Stream：接收 `tokio::io::AsyncBufRead`，逐行解析
- 损坏行跳过：JSON 解析失败时 tracing::warn! + 继续（不 panic）
- SESSION_ID 锁定：第一次出现即锁定，后续事件不覆盖
- 未知格式行静默跳过（兼容未来新字段）

**PBT 属性**:
- 幂等性：同一段 Stream 多次解析 → 产出相同事件序列
- 鲁棒性：随机二进制数据混入 JSON 行 → 解析器不 panic
- 完整性：所有合法 JSON 行都被解析为事件（不遗漏）
- SESSION_ID 单调性：一旦锁定，final session_id == 首次出现的值

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-router/tests/stream_test.rs
   - #[tokio::test] fn parse_codex_thread_started()
     → 输入 {"thread_id":"xxx","type":"thread.started"} → 事件 Init + session_id
   - #[tokio::test] fn parse_codex_agent_message()
     → 输入 Codex item.completed (agent_message) → 事件 AgentMessage
   - #[tokio::test] fn parse_claude_result()
     → 输入 {"type":"result","result":"code here","session_id":"yyy"} → 事件 AgentMessage + session_id
   - #[tokio::test] fn parse_gemini_content_delta()
     → 输入 {"role":"assistant","content":"hello","delta":true} → 事件 Progress
   - #[tokio::test] fn parse_gemini_complete()
     → 输入 {"status":"success"} → 事件 Complete
   - #[tokio::test] fn session_id_locked_on_first_occurrence()
     → 输入两个含不同 session_id 的事件 → 最终 session_id == 第一个
   - #[tokio::test] fn malformed_json_skipped()
     → 输入 "not json\n{}" → 仅产出 1 个事件
   - proptest! fn random_bytes_no_panic(data in "\\PC{0,500}")
     → 解析任意数据不 panic
   → cargo test → 全部编译失败

🟢 Green: 实现 StreamParser + parse_line() + parse_stream()
   → cargo test → 全部通过

🔵 Refactor: 将后端检测逻辑提取为 detect_backend_from_json() 函数
   → cargo test + clippy → 零警告
```

---

#### T23 - SESSION_ID 管理器

**前置依赖**: T22
**产出**: `crates/ghostcode-router/src/session.rs`
**参考**: `ccg-workflow/codeagent-wrapper/parser.go:160-163`, `main.go:483-485`

**公共 API**:

```rust
/// Session 存储（持久化到文件）
struct SessionStore {
    sessions: RwLock<HashMap<SessionKey, String>>,
    file_path: PathBuf,
}

/// Session 键：(group_id, actor_id, backend_name)
type SessionKey = (String, String, String);

impl SessionStore {
    /// 创建/加载 SessionStore
    fn new(file_path: PathBuf) -> Result<Self>;

    /// 保存 session_id
    fn save(&self, key: SessionKey, session_id: String) -> Result<()>;

    /// 获取 session_id
    fn get(&self, key: &SessionKey) -> Option<String>;

    /// 列出所有 session
    fn list(&self) -> Vec<(SessionKey, String)>;

    /// 持久化到文件
    fn flush(&self) -> Result<()>;
}
```

**约束**:
- Session 文件位置：`<group_dir>/state/sessions.json`
- 线程安全：RwLock 保护内存状态
- 持久化策略：save() 时先更新内存，再异步 flush 到文件
- 文件格式：`{"<group_id>/<actor_id>/<backend>": "<session_id>"}`
- 同一 key 的新 session_id 覆盖旧值

**PBT 属性**:
- 往返性：save N 个 session → flush → 重新 load → 所有 session_id 完整恢复
- 唯一性：同一 key 多次 save → get 返回最后一次的值
- 隔离性：不同 key 的 session 互不影响
- 线程安全：并发 save + get 不 panic

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-router/tests/session_test.rs
   - #[test] fn save_and_get_session()
     → save("g1","a1","codex","sid1") → get == Some("sid1")
   - #[test] fn save_overwrites_old()
     → save 两次不同 sid → get 返回最新
   - #[test] fn different_backends_isolated()
     → save("g1","a1","codex","sid1") + save("g1","a1","gemini","sid2") → 各自独立
   - #[test] fn flush_and_reload()
     → save → flush → 新建 SessionStore 从文件加载 → get 一致
   - #[test] fn list_all_sessions()
     → save 3 个 → list 返回 3 个
   - proptest! fn roundtrip_persistence(
         entries in prop::collection::vec(arb_session_entry(), 1..50)
     )
     → save all → flush → reload → 全部 get 一致
   → cargo test → 全部编译失败

🟢 Green: 实现 SessionStore
   → cargo test → 全部通过

🔵 Refactor: 使用 serde 宏简化 JSON 序列化
   → cargo test + clippy → 零警告
```

---

#### T24 - ROLE_FILE 注入器

**前置依赖**: T21
**产出**: `crates/ghostcode-router/src/rolefile.rs`
**参考**: `ccg-workflow/codeagent-wrapper/utils.go:75-117`

**公共 API**:

```rust
/// 注入 ROLE_FILE 引用
///
/// 扫描文本中的 `ROLE_FILE: <path>` 行，读取文件内容替换整行
/// 支持多个 ROLE_FILE 引用
fn inject_role_files(text: &str) -> Result<String>;
```

**实现细节**:
- 正则：`(?m)^ROLE_FILE:\s*(.+)$`
- 匹配后读取 path 指向的文件全文，替换整行
- 文件不存在 → 返回 Error（不静默跳过）
- 文件大小上限 1MB（防止 Token 炸弹）
- 路径支持 `~` 展开和相对路径解析

**约束**:
- 文件路径两端 trim 空白
- 多个 ROLE_FILE 引用按出现顺序逐一替换
- 替换后的文本不再包含 "ROLE_FILE:" 前缀

**PBT 属性**:
- 无 ROLE_FILE 引用 → 输出等于输入
- 注入后文本不包含 "ROLE_FILE:" 前缀
- 文件大小 > 1MB → 返回 FileTooLarge 错误
- 注入后文本长度 >= 原文本长度（替换后不可能变短，除非文件为空）

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-router/tests/rolefile_test.rs
   - #[test] fn inject_single_rolefile()
     → 创建临时文件 → ROLE_FILE: <path> → 被文件内容替换
   - #[test] fn inject_multiple_rolefiles()
     → 两个 ROLE_FILE 引用 → 各自被替换
   - #[test] fn no_rolefile_unchanged()
     → 无 ROLE_FILE → 输出等于输入
   - #[test] fn missing_file_returns_error()
     → ROLE_FILE: /nonexistent → 返回 Error
   - #[test] fn oversized_file_returns_error()
     → 创建 2MB 文件 → 返回 FileTooLarge
   - #[test] fn whitespace_trimmed_from_path()
     → ROLE_FILE:   <path>   → 路径正确解析
   - proptest! fn no_rolefile_idempotent(text in "[^R]{0,200}")
     → 不含 ROLE_FILE 的文本注入后不变
   → cargo test → 全部编译失败

🟢 Green: 实现 inject_role_files()
   → cargo test → 全部通过

🔵 Refactor: 将文件读取提取为可 mock 的 trait
   → cargo test + clippy → 零警告
```

---

#### T25 - 异步子进程管理器

**前置依赖**: T21, T22
**产出**: `crates/ghostcode-router/src/process.rs`
**参考**: `ccg-workflow/codeagent-wrapper/executor.go:810-1280`

**公共 API**:

```rust
/// 子进程执行结果
struct ProcessResult {
    exit_code: i32,
    stdout_events: Vec<StreamEvent>, // 解析后的事件流
    stderr: String,                   // 过滤后的 stderr
    session_id: Option<String>,       // 提取的 SESSION_ID
    duration: Duration,               // 执行耗时
}

/// 子进程管理器
struct ProcessManager;

impl ProcessManager {
    /// 启动子进程并等待完成
    async fn run(
        backend: &dyn Backend,
        config: &TaskConfig,
        task_text: &str,
        cancel: CancellationToken,
    ) -> Result<ProcessResult>;
}
```

**实现细节**:
- stdin 决策：任务文本含 `\n \\ " ' \` $` 或长度 > 800 字节 → 使用 stdin 管道
  - 参考: ccg-workflow/codeagent-wrapper/utils.go:50-58
- stdout 管道：逐行读取 → 传给 StreamParser 解析
- stderr 管道：过滤噪声日志（模式匹配移除）
- 超时控制：tokio::time::timeout 包裹
- 信号处理：超时/取消 → SIGTERM → 等待 5s → SIGKILL
  - 参考: executor.go:1282-1318
- 环境变量：CODEX_TIMEOUT, GHOSTCODE_BACKEND 等

**约束**:
- 子进程退出后所有 fd 关闭（无泄漏）
- SIGTERM 后等待 5 秒才发 SIGKILL（给子进程清理时间）
- 工作目录通过 cmd.current_dir() 设置
- 进程退出码非零 → 返回 ProcessFailed 错误（附带 stderr）

**PBT 属性**:
- stdin 决策正确性：含特殊字符的文本或 > 800 字节 → should_use_stdin() == true
- 短文本无特殊字符 → should_use_stdin() == false
- 超时后子进程必定被终止（进程表中不残留）

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-router/tests/process_test.rs
   - #[tokio::test] fn run_echo_captures_stdout()
     → 执行 echo '{"type":"test"}' → 解析为事件
   - #[tokio::test] fn run_cat_stdin_mode()
     → 长文本 (>800 bytes) 通过 stdin 传递给 cat → stdout 一致
   - #[tokio::test] fn stdin_for_special_chars()
     → 文本含 \n → should_use_stdin() == true
   - #[tokio::test] fn args_mode_for_short_text()
     → 短文本无特殊字符 → should_use_stdin() == false
   - #[tokio::test] fn timeout_kills_process()
     → 执行 sleep 100 + 1s 超时 → 子进程被终止
   - #[tokio::test] fn cancel_stops_process()
     → 执行 sleep 100 + cancel token → 子进程被终止
   - #[tokio::test] fn nonzero_exit_returns_error()
     → 执行 false → 返回 ProcessFailed
   → cargo test → 全部编译失败

🟢 Green: 实现 ProcessManager::run() + should_use_stdin()
   → cargo test → 全部通过

🔵 Refactor: 提取信号处理逻辑为 terminate_process()
   → cargo test + clippy → 零警告
```

---

### 阶段 F：执行引擎

#### T26 - DAG 拓扑排序

**前置依赖**: 无（纯算法，可与 T21 并行）
**产出**: `crates/ghostcode-router/src/dag.rs`
**参考**: `ccg-workflow/codeagent-wrapper/executor.go:287-351`

**公共 API**:

```rust
/// 任务规格（轻量，用于 DAG 排序）
struct TaskNode {
    id: String,
    dependencies: Vec<String>,
}

/// DAG 排序错误
enum DagError {
    CycleDetected(Vec<String>),  // 环中的节点 ID 列表
    MissingDependency { task: String, missing: String },
}

/// BFS 拓扑排序
/// 返回层数组：每层可并行执行，层间串行
fn topological_sort(tasks: Vec<TaskNode>) -> Result<Vec<Vec<TaskNode>>, DagError>;
```

**实现细节**:
- BFS 层遍历算法：
  1. 建入度表 + 邻接表
  2. 入度为 0 的节点入队（第一层）
  3. 弹出节点后降低后继入度，新的 0 入度节点入下一层队列
  4. 环检测：processed != len(tasks) → CycleDetected
- 缺失依赖检测：依赖的 task_id 不存在 → MissingDependency

**约束**:
- 空输入 → 返回空层数组（不报错）
- 单任务无依赖 → 一层含一个任务
- 同一层内任务顺序不保证（可并行）

**PBT 属性**:
- 完备性：输出层数组包含所有输入任务（flatten 后长度相等）
- 拓扑序：每个任务的所有依赖在更早的层
- 环检测：含环图 → 返回 CycleDetected
- 无依赖任务全在第一层
- 线性链 A→B→C → 产出 3 层各 1 个任务

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-router/tests/dag_test.rs
   - #[test] fn empty_input_empty_output()
   - #[test] fn single_task_one_layer()
   - #[test] fn independent_tasks_all_first_layer()
     → 3 个无依赖任务 → 1 层含 3 个
   - #[test] fn linear_chain_n_layers()
     → A→B→C → 3 层各 1 个
   - #[test] fn diamond_dependency()
     → A→B, A→C, B→D, C→D → 3 层: [A], [B,C], [D]
   - #[test] fn cycle_detected()
     → A→B→C→A → CycleDetected
   - #[test] fn missing_dependency_detected()
     → A depends on X（X 不存在） → MissingDependency
   - proptest! fn topo_sort_complete(tasks in arb_dag(1..50))
     → flatten(result) 长度 == 输入长度
   - proptest! fn topo_sort_valid_order(tasks in arb_dag(1..50))
     → 每个任务的依赖都在更早层
   → cargo test → 全部编译失败

🟢 Green: 实现 topological_sort()
   → cargo test → 全部通过

🔵 Refactor: 优化邻接表构建，使用 HashMap<&str, Vec<&str>>
   → cargo test + clippy → 零警告
```

---

#### T27 - 并行执行引擎

**前置依赖**: T25 (子进程管理), T26 (DAG)
**产出**: `crates/ghostcode-router/src/executor.rs`
**参考**: `ccg-workflow/codeagent-wrapper/executor.go:353-515`

**公共 API**:

```rust
/// 任务规格（完整，用于执行）
struct TaskSpec {
    id: String,
    task_text: String,
    workdir: PathBuf,
    backend: String,           // "codex"/"claude"/"gemini"
    mode: TaskMode,
    session_id: Option<String>,
    dependencies: Vec<String>,
}

/// 任务结果
struct TaskResult {
    id: String,
    status: TaskStatus,        // Success / Failed / Skipped / Cancelled
    process_result: Option<ProcessResult>,
    duration: Duration,
}

enum TaskStatus {
    Success,
    Failed(String),     // 错误信息
    Skipped(String),    // 跳过原因（依赖失败）
    Cancelled,
}

/// 执行引擎配置
struct ExecutorConfig {
    max_workers: usize,         // 最大并行数（0=无限制，最大 100）
    cancel: CancellationToken,  // 取消令牌
}

/// 执行引擎
struct Executor;

impl Executor {
    /// 执行 DAG 任务集
    async fn run(
        tasks: Vec<TaskSpec>,
        backends: &BackendRegistry,
        config: ExecutorConfig,
    ) -> Vec<TaskResult>;
}
```

**实现细节**:
- 层间串行、层内并行：
  ```
  for layer in topological_sort(tasks) {
      let results = join_all(layer.map(|task| {
          semaphore.acquire() → execute_task(task)
      })).await;
      // 检查失败，标记后续依赖为 Skipped
  }
  ```
- 信号量：`tokio::sync::Semaphore::new(max_workers.min(100))`
- 依赖失败传播：任务 A 失败 → 所有直接/间接依赖 A 的任务标记为 Skipped
- 取消传播：CancellationToken 触发 → 所有正在执行的任务收到取消信号

**约束**:
- max_workers = 0 → 无限制（实际最大 100）
- 空任务列表 → 返回空结果列表
- 单任务失败不影响同层其他任务
- 所有任务都有结果（无任务被遗漏）

**PBT 属性**:
- 完整性：结果数量 == 输入任务数量
- 并行度：同时运行的任务数 <= max_workers
- 因果性：依赖任务失败 → 所有后续依赖被标记为 Skipped
- 层序性：Layer N 全部完成后才开始 Layer N+1

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-router/tests/executor_test.rs
   - #[tokio::test] fn single_task_success()
     → 提交 echo 任务 → 状态 Success
   - #[tokio::test] fn parallel_tasks_concurrent()
     → 3 个独立 sleep 0.5s 任务 → 总耗时 < 2s（并行证明）
   - #[tokio::test] fn dependency_failure_skips_dependents()
     → A(fail) → B → B 状态 Skipped
   - #[tokio::test] fn semaphore_limits_concurrency()
     → 5 个任务 + max_workers=2 → 同时最多 2 个运行
   - #[tokio::test] fn cancel_stops_all()
     → 提交长任务 + 100ms 后取消 → 所有任务 Cancelled
   - #[tokio::test] fn empty_tasks_empty_results()
   - #[tokio::test] fn same_layer_failure_no_cross_impact()
     → 同层 A(fail) + B(ok) → B 仍 Success
   → cargo test → 全部编译失败

🟢 Green: 实现 Executor::run()
   → cargo test → 全部通过

🔵 Refactor: 提取 execute_layer() 和 propagate_failures()
   → cargo test + clippy → 零警告
```

---

#### T28 - 任务格式解析器

**前置依赖**: T21
**产出**: `crates/ghostcode-router/src/task_format.rs`
**参考**: `ccg-workflow/codeagent-wrapper/config.go:113-185`

**公共 API**:

```rust
/// 解析 ---TASK---/---CONTENT--- 格式的文本
fn parse_task_format(input: &str) -> Result<Vec<TaskSpec>>;

/// 序列化 TaskSpec 列表为标准格式
fn serialize_task_format(tasks: &[TaskSpec]) -> String;
```

**格式定义**:
```
---TASK---
id: task1
workdir: /path/to/dir
backend: codex
dependencies: dep1,dep2
session_id: xxx123
---CONTENT---
实际任务内容
可以是多行
---TASK---
id: task2
---CONTENT---
另一个任务
```

**约束**:
- 必填字段：id
- 可选字段：workdir, backend（默认 "codex"）, dependencies（默认空）, session_id
- session_id 存在时自动设置 mode = Resume
- 依赖以逗号分隔，trim 空白
- ---CONTENT--- 之后到下一个 ---TASK--- 或 EOF 之间的所有文本为 task_text
- 空输入 → 返回空 Vec

**PBT 属性**:
- 往返性：Vec<TaskSpec> → serialize → parse → 逐字段相等
- 完整性：N 个 ---TASK--- 块 → 解析出 N 个 TaskSpec
- 鲁棒性：缺少可选字段不 panic（使用默认值）
- 内容保真性：task_text 中的换行、空格、特殊字符完整保留

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-router/tests/task_format_test.rs
   - #[test] fn parse_single_task()
   - #[test] fn parse_multiple_tasks()
   - #[test] fn parse_with_dependencies()
     → dependencies: a,b → vec!["a","b"]
   - #[test] fn session_id_sets_resume_mode()
   - #[test] fn missing_optional_fields_use_defaults()
   - #[test] fn multiline_content_preserved()
   - #[test] fn empty_input_empty_result()
   - proptest! fn roundtrip(tasks in arb_task_specs(1..10))
     → serialize → parse → 相等
   → cargo test → 全部编译失败

🟢 Green: 实现 parse_task_format() + serialize_task_format()
   → cargo test → 全部通过

🔵 Refactor: 提取 parse_header_field() 辅助函数
   → cargo test + clippy → 零警告
```

---

### 阶段 G：代码主权

#### T29 - 写入权限守卫

**前置依赖**: T21
**产出**: `crates/ghostcode-router/src/sovereignty.rs`
**参考**: `ccg-workflow/templates/commands/execute.md:208-237`, `README.md:221`

**公共 API**:

```rust
/// 代码主权守卫
struct SovereigntyGuard {
    write_actor: String,  // 允许写入的 Actor 身份（默认 "claude"）
}

/// 操作审查结果
enum ReviewResult {
    Approved,                          // 允许执行
    NeedsReview { suggestion: String }, // 需要 Claude 审核后执行
    Rejected { reason: String },       // 直接拒绝
}

impl SovereigntyGuard {
    /// 创建守卫（从环境变量或默认值）
    fn new() -> Self;

    /// 检查后端是否有写入权限
    fn can_write(&self, backend_name: &str) -> bool;

    /// 审查外部模型输出
    fn review_output(&self, backend_name: &str, output: &str) -> ReviewResult;
}
```

**实现细节**:
- 核心规则：只有 `write_actor` 指定的后端可以直接写入文件
- 配置来源：环境变量 `GHOSTCODE_WRITE_ACTOR`，默认 "claude"
- 外部模型输出处理流程：
  1. 后端是 Claude → Approved（直接执行）
  2. 后端是 Codex/Gemini → NeedsReview（返回建议文本，由 Claude 审核后应用）
  3. 输出中检测到危险操作（rm -rf, drop table 等） → Rejected
- 参考 ccg-workflow 的 5 步审核流程：读取 Diff → 思维沙箱 → 重构清理 → 最小作用域 → 应用变更

**约束**:
- 默认 write_actor = "claude"（不可为空）
- can_write() 大小写不敏感比较
- 危险操作检测为硬编码模式列表（初版简单实现）

**PBT 属性**:
- 不变性：非 write_actor 身份 → can_write() == false
- Claude 始终可写：can_write("claude") == true（默认配置）
- 配置生效：自定义 write_actor → 仅该身份可写
- 空字符串后端名 → can_write() == false

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-router/tests/sovereignty_test.rs
   - #[test] fn claude_can_write()
   - #[test] fn codex_cannot_write()
   - #[test] fn gemini_cannot_write()
   - #[test] fn custom_write_actor()
     → GHOSTCODE_WRITE_ACTOR=codex → codex 可写, claude 不可写
   - #[test] fn case_insensitive()
     → can_write("Claude") == can_write("claude")
   - #[test] fn codex_output_needs_review()
     → review_output("codex", "some code") → NeedsReview
   - #[test] fn claude_output_approved()
     → review_output("claude", "some code") → Approved
   - #[test] fn dangerous_output_rejected()
     → review_output("codex", "rm -rf /") → Rejected
   - proptest! fn non_write_actor_always_false(
         name in "[a-z]{3,10}".prop_filter("not claude", |s| s != "claude")
     )
     → can_write(name) == false（默认配置下）
   → cargo test → 全部编译失败

🟢 Green: 实现 SovereigntyGuard
   → cargo test → 全部通过

🔵 Refactor: 将 can_write() 提取为纯函数
   → cargo test + clippy → 零警告
```

---

#### T30 - Daemon 路由 Dispatch 集成

**前置依赖**: T21, T23, T27, T29
**产出**: 修改 `crates/ghostcode-daemon/src/dispatch.rs`
**参考**: Phase 1 dispatch 模式

**新增 dispatch op**:

| op | 参数 | 返回 | 功能 |
|----|------|------|------|
| `route_task` | group_id, task_text, backend, workdir | task_id | 提交单任务到路由引擎 |
| `route_task_parallel` | group_id, tasks_format | task_id | 提交并行任务集（DAG） |
| `route_status` | group_id, task_id | status, progress | 查询任务执行状态 |
| `route_cancel` | group_id, task_id | ok | 取消正在执行的任务 |
| `session_list` | group_id | sessions[] | 列出所有已保存的 session |

**文件范围**:
- `crates/ghostcode-daemon/src/dispatch.rs` (修改：追加 5 个 match arm)
- `crates/ghostcode-daemon/src/routing.rs` (新建：路由状态管理)
- `crates/ghostcode-daemon/tests/router_dispatch_test.rs` (新建)

**约束**:
- 所有新 op 的 ID 参数通过 validate_id() 校验（复用 Phase 1 安全模式）
- route_task 返回唯一 task_id（UUID）
- route_status 支持轮询（幂等查询）
- route_cancel 幂等（重复取消不报错）
- handler 逻辑委托给 ghostcode-router crate，dispatch 仅做参数提取和转发

**PBT 属性**:
- 所有新 op 通过 validate_id()（恶意 ID 被拒绝）
- 未知 op 返回 unknown_op 错误（不影响已有 21 个 op）
- route_task 返回的 task_id 可用于 route_status 查询

**TDD 步骤**:
```
🔴 Red: crates/ghostcode-daemon/tests/router_dispatch_test.rs
   - #[tokio::test] fn route_task_returns_task_id()
   - #[tokio::test] fn route_status_returns_pending_then_running()
   - #[tokio::test] fn route_cancel_stops_task()
   - #[tokio::test] fn session_list_returns_saved()
   - #[tokio::test] fn route_parallel_accepts_dag_format()
   - #[tokio::test] fn invalid_group_id_rejected()
   - #[tokio::test] fn existing_ops_unaffected()
     → ping, actor_start 等仍正常工作
   → cargo test → 全部编译失败

🟢 Green: 在 dispatch() 追加 5 个 match arm + routing.rs 状态管理
   → cargo test → 全部通过

🔵 Refactor: handler 逻辑完全委托给 ghostcode-router
   → cargo test + clippy → 零警告
```

---

### 阶段 H：Plugin 集成 (TypeScript)

#### T31 - 路由决策逻辑 (Plugin 层)

**前置依赖**: T30
**产出**: `src/plugin/src/router/`
**参考**: `ccg-workflow/src/types/index.ts (ModelRouting)`

**需要定义的类型**:

```typescript
/** 模型路由配置 */
interface ModelRouting {
  frontend: {
    primary: 'gemini';
    strategy: RoutingStrategy;
  };
  backend: {
    primary: 'codex';
    strategy: RoutingStrategy;
  };
  mode: 'smart' | 'parallel' | 'sequential';
}

type RoutingStrategy = 'parallel' | 'fallback' | 'round-robin';

/** 路由决策结果 */
interface RouteDecision {
  backend: 'codex' | 'claude' | 'gemini';
  reason: string;      // 路由原因（用于透明度显示）
  confidence: number;  // 置信度 0-1
}
```

**文件范围**:
- `src/plugin/src/router/index.ts` (新建：模块导出)
- `src/plugin/src/router/types.ts` (新建：类型定义)
- `src/plugin/src/router/strategy.ts` (新建：路由策略实现)
- `src/plugin/src/router/__tests__/strategy.test.ts` (新建)

**约束**:
- 关键词路由规则：
  - 前端标识：css, html, ui, ux, style, component, layout, responsive, design, animation
  - 后端标识：api, database, db, sql, algorithm, server, backend, logic, auth, middleware
  - 未匹配 → 默认 claude
- 强制前缀覆盖：`/codex ...` 或 `/gemini ...` 或 `/claude ...`
- 路由策略优先级：强制前缀 > 关键词匹配 > 默认值

**TDD 步骤**:
```
🔴 Red: src/plugin/src/router/__tests__/strategy.test.ts
   - test("CSS task routes to gemini")
   - test("API task routes to codex")
   - test("modify database routes to codex")
   - test("responsive design routes to gemini")
   - test("explicit /codex prefix overrides auto")
   - test("explicit /gemini prefix overrides auto")
   - test("unknown task defaults to claude")
   - test("empty task defaults to claude")
   - test("confidence > 0 for keyword match")
   - test("confidence == 1 for forced prefix")
   → pnpm test → 全部失败

🟢 Green: 实现 resolveRoute()
   → pnpm test → 全部通过

🔵 Refactor: 将关键词规则提取为可配置的 Map
   → pnpm test + pnpm build → 通过
```

---

#### T32 - 命令模板引擎

**前置依赖**: T31
**产出**: `src/plugin/src/router/templates.ts`
**参考**: `ccg-workflow/templates/commands/` 的 9 个命令模板

**公共 API**:

```typescript
/** 模板变量 */
interface TemplateVars {
  WORKDIR: string;
  BACKEND: string;
  MODEL?: string;
  LITE_MODE_FLAG?: string;
  TASK: string;
}

/** 渲染命令模板 */
function renderTemplate(template: string, vars: TemplateVars): string;

/** 加载并渲染预定义模板 */
function loadTemplate(name: string, vars: TemplateVars): string;
```

**文件范围**:
- `src/plugin/src/router/templates.ts` (新建)
- `src/plugin/src/router/__tests__/templates.test.ts` (新建)

**约束**:
- 变量语法：`{{VARIABLE_NAME}}`
- 每个模板自动注入代码主权声明：`外部模型对文件系统零写入权限，所有修改由 Claude 执行`
- ROLE_FILE 引用在模板中保留（由 Rust 侧 T24 在执行前注入）
- 未知变量保留原样（不报错）

**TDD 步骤**:
```
🔴 Red: src/plugin/src/router/__tests__/templates.test.ts
   - test("renders workdir variable")
   - test("renders backend variable")
   - test("includes sovereignty declaration")
   - test("preserves ROLE_FILE reference")
   - test("unknown variable left unchanged")
   - test("empty template returns sovereignty header only")
   - test("multiple variables in one template")
   → pnpm test → 全部失败

🟢 Green: 实现 renderTemplate() + loadTemplate()
   → pnpm test → 全部通过

🔵 Refactor: 模板内容从字符串常量改为文件加载
   → pnpm test + pnpm build → 通过
```

---

#### T33 - 流式输出处理器

**前置依赖**: T30, T31
**产出**: `src/plugin/src/router/streaming.ts`
**参考**: Phase 1 ipc.ts 的 callDaemon() 模式

**公共 API**:

```typescript
/** 流式事件回调 */
interface StreamCallbacks {
  onProgress?: (event: StreamEvent) => void;
  onAgentMessage?: (content: string, backend: string) => void;
  onComplete?: (result: TaskResult) => void;
  onError?: (error: Error) => void;
}

/** 流式响应处理器 */
class StreamingHandler {
  constructor(callbacks: StreamCallbacks);
  /** 处理 IPC 推送的一行 JSON */
  handleLine(line: string): void;
  /** 获取提取的 SESSION_ID */
  getSessionId(): string | undefined;
}
```

**文件范围**:
- `src/plugin/src/router/streaming.ts` (新建)
- `src/plugin/src/router/__tests__/streaming.test.ts` (新建)
- `src/plugin/src/ipc.ts` (修改：新增 callDaemonStream() 方法)

**约束**:
- IPC 流式协议：Daemon 通过已有连接逐行推送 JSON 事件
- callDaemonStream() 与 callDaemon() 并存（不破坏已有接口）
- SESSION_ID 自动提取并缓存
- 错误事件触发 onError 回调后不中断流（继续处理后续事件）

**TDD 步骤**:
```
🔴 Red: src/plugin/src/router/__tests__/streaming.test.ts
   - test("handleLine parses valid JSON event")
   - test("handleLine ignores malformed JSON")
   - test("onAgentMessage callback fires for agent output")
   - test("onComplete callback fires on finish event")
   - test("onError callback fires on error event")
   - test("session_id extracted from first event")
   - test("session_id not overwritten by later events")
   - test("multiple lines processed sequentially")
   → pnpm test → 全部失败

🟢 Green: 实现 StreamingHandler + callDaemonStream()
   → pnpm test → 全部通过

🔵 Refactor: 将事件解析逻辑与回调分离
   → pnpm test + pnpm build → 通过
```

---

### 阶段 I：MCP 工具 + 端到端集成

#### T34 - 路由 MCP 工具实现

**前置依赖**: T30
**产出**: `crates/ghostcode-mcp/src/tools/` 新增 3 个工具
**参考**: Phase 1 MCP 工具模式

**新增 MCP 工具**:

| 工具名 | 参数 | 功能 |
|--------|------|------|
| `ghostcode_route_task` | group_id, task, backend?, workdir? | 提交任务到路由引擎 |
| `ghostcode_route_status` | group_id, task_id | 查询任务执行状态 |
| `ghostcode_route_cancel` | group_id, task_id | 取消正在执行的任务 |

**文件范围**:
- `crates/ghostcode-mcp/src/tools/route_task.rs` (新建)
- `crates/ghostcode-mcp/src/tools/route_status.rs` (新建)
- `crates/ghostcode-mcp/src/tools/route_cancel.rs` (新建)
- `crates/ghostcode-mcp/src/tools/mod.rs` (修改：注册新工具)

**约束**:
- JSON Schema 与 Phase 1 工具保持一致的风格
- 每个工具有 description + inputSchema
- 通过 IPC 调用 Daemon 的 route_* op
- 返回格式与已有工具一致（{content: [{type: "text", text: "..."}]}）

**TDD 步骤**:
```
🔴 Red: （在 tests/ 中验证 schema 有效性）
   - #[test] fn route_task_schema_valid()
   - #[test] fn route_status_schema_valid()
   - #[test] fn route_cancel_schema_valid()
   - #[test] fn route_task_requires_group_id()
   - #[test] fn all_tools_registered_in_mod()
   → cargo test → 全部编译失败

🟢 Green: 实现 3 个 MCP 工具
   → cargo test → 全部通过

🔵 Refactor: 与已有工具保持代码结构一致
   → cargo test + clippy → 零警告
```

---

#### T35 - 多模型端到端集成测试

**前置依赖**: T30, T33, T34
**产出**: 集成测试文件
**参考**: Phase 1 T19 集成测试模式

**文件范围**:
- `crates/ghostcode-daemon/tests/router_integration_test.rs` (新建)
- `src/plugin/src/router/__tests__/integration.test.ts` (新建)

**集成测试场景**:

```
场景 1：单任务路由
  → 提交 CSS 相关任务
  → 路由到 Gemini 后端（mock）
  → 返回建议文本
  → 状态 NeedsReview

场景 2：DAG 并行执行
  → 提交 5 个任务，含 A→B, A→C, B→D, C→D 依赖
  → DAG 排序：[A], [B,C], [D]
  → 并行执行层 [B,C]
  → 全部完成

场景 3：代码主权守卫
  → 提交任务到 Codex 后端
  → 输出包含 "write file" 建议
  → SovereigntyGuard 标记为 NeedsReview
  → 不直接写入文件系统

场景 4：SESSION_ID 复用
  → 执行任务 → 获取 session_id
  → 以 resume 模式再次执行 → 参数包含 session_id
  → 上下文保留

场景 5：任务取消
  → 提交长时间任务（mock sleep）
  → 调用 route_cancel
  → 子进程被终止
  → 状态 Cancelled
```

**约束**:
- 使用 mock 后端（echo 命令）替代真实 CLI（Codex/Gemini 可能不可用）
- 测试不依赖网络和外部服务
- Rust 集成测试和 TS 集成测试可独立运行

**TDD 步骤**:
```
🔴 Red:
   Rust 侧:
   - #[tokio::test] fn e2e_single_task_route()
   - #[tokio::test] fn e2e_dag_parallel_execution()
   - #[tokio::test] fn e2e_sovereignty_blocks_write()
   - #[tokio::test] fn e2e_session_resume()
   - #[tokio::test] fn e2e_task_cancel()

   TS 侧:
   - test("plugin routes task through IPC")
   - test("streaming events received")
   - test("session_id cached after execution")
   → cargo test + pnpm test → 全部编译失败

🟢 Green: 搭建 mock 后端 + 实现集成测试
   → cargo test + pnpm test → 全部通过

🔵 Refactor: 提取 mock 后端到 testutil 模块
   → cargo test + pnpm test + clippy → 零警告
```

---

## 依赖关系图

```
阶段 E（路由核心）:
  T21 (Backend Trait)  ─┬──→ T22 (Stream Parser) ──→ T23 (Session)
                        ├──→ T24 (ROLE_FILE)       ↘
                        ├──→ T28 (Task Format)      T25 (Process)
                        └──→ T29 (Sovereignty)         ↓
  T26 (DAG Sort) ──────────────────────────────────→ T27 (Executor)
                                                        ↓
阶段 G（代码主权）:                                     ↓
  T29 ──────────────────────────────────────────────→ T30 (Dispatch)
                                                        ↓
阶段 H（Plugin 集成）:                              ┌── T31 (Route Logic)
                                                    │   ↓
  T30 ─────────────────────────────────────────────┼── T32 (Templates)
                                                    │   T33 (Streaming)
                                                    └── T34 (MCP Tools)
                                                        ↓
阶段 I（端到端）:                                    T35 (E2E Test)
```

## 并行分组

```
Layer 1 (并行): T21, T24, T26
Layer 2 (依赖 T21): T22, T28, T29
Layer 3 (依赖 T22): T23, T25
Layer 4 (依赖 T25+T26): T27
Layer 5 (依赖 T27+T29): T30
Layer 6 (依赖 T30): T31, T34
Layer 7 (依赖 T31): T32, T33
Layer 8 (依赖 T30+T33+T34): T35
```

---

## 代码量估算

| 阶段 | 任务 | Rust 行数 | TS 行数 | 测试行数 |
|------|------|----------|---------|---------|
| E 路由核心 | T21-T25 | ~1800 | - | ~1200 |
| F 执行引擎 | T26-T28 | ~1000 | - | ~800 |
| G 代码主权 | T29-T30 | ~600 | - | ~500 |
| H Plugin | T31-T33 | - | ~900 | ~600 |
| I 集成 | T34-T35 | ~400 | ~200 | ~800 |
| **总计** | **15** | **~3800** | **~1100** | **~3900** |

总代码量约 **8800 行**（含测试）。

---

## 风险评估

| 风险 | 级别 | 影响任务 | 缓解策略 |
|------|------|---------|---------|
| 子进程信号处理的 macOS 特殊行为 | 高 | T25 | Phase 2 仅支持 macOS，参考 ccg-workflow 的信号转发逻辑 |
| 外部 CLI（codex/gemini）版本不兼容 | 中 | T22, T35 | JSON Stream 格式用 serde(default) 兼容新字段 |
| DAG 执行引擎的并发死锁 | 中 | T27 | 严格层间串行 + 层内并行，无跨层锁 |
| IPC streaming 背压 | 中 | T33 | 事件缓冲区上限 + 丢弃最旧策略 |
| ROLE_FILE 导致 Token 窗口溢出 | 低 | T24 | 文件大小上限 1MB |
| Codex CLI 不稳定（频繁 exit 1） | 高 | T35 | mock 后端做集成测试，真实 CLI 做手动验收 |

---

*Phase 2 任务清单完成：2026-03-02*
*分析来源：Gemini 前端分析 + Claude 综合研判 + 参考项目源码验证*
*Codex 后端分析：失败（codex exited with status 1），由 Claude 基于源码研究补充*
*下一步：按阶段 E → I 顺序逐个实施*
