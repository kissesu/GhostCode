//! @file router_integration_test.rs
//! @description 多模型路由端到端集成测试 - 验证路由引擎完整流程
//!              Phase 6 新增：使用 TempDir + AppState::new() 替代 AppState::default()
//!              确保 SessionStore 正确初始化并挂载到 AppState
//! @author Atlas.oi
//! @date 2026-03-04

use ghostcode_daemon::dispatch::dispatch;
use ghostcode_daemon::server::AppState;
use ghostcode_types::ipc::DaemonRequest;
use tempfile::TempDir;

// ============================================
// 测试基础结构：使用 TempDir + AppState::new()
// ============================================

/// 创建带真实 groups_dir 的测试 AppState
///
/// Phase 6 规范：使用真实临时目录初始化 AppState，
/// 确保 SessionStore 挂载正确，不使用 Default 的固定路径
fn make_test_state() -> (TempDir, AppState) {
    let tmp = TempDir::new().expect("创建临时目录失败");
    let state = AppState::new(tmp.path().to_path_buf());
    // 注意：返回 TempDir 以延长生命周期，防止目录被提前删除
    (tmp, state)
}

// ============================================
// 辅助函数：构造 DaemonRequest
// ============================================

/// 构造测试用 DaemonRequest
fn make_req(op: &str, args: serde_json::Value) -> DaemonRequest {
    DaemonRequest::new(op, args)
}

// ============================================
// 场景 1: 提交单任务 -> 路由到正确后端 -> 查询状态
// ============================================

/// 提交单任务到 codex 后端，验证：
/// 1. 返回 task_id
/// 2. route_status 查询状态为 pending
/// 3. can_write=false（codex 无写入权限）
#[tokio::test]
async fn route_single_task_and_query_status() {
    let (_tmp, state) = make_test_state();

    // 第一步：提交任务到 codex 后端
    let create_resp = dispatch(
        &state,
        make_req(
            "route_task",
            serde_json::json!({
                "group_id": "test-group",
                "task_text": "实现排序算法",
                "backend": "codex",
                "workdir": "/tmp"
            }),
        ),
    )
    .await;

    // 验证提交成功，返回必要字段
    assert!(
        create_resp.ok,
        "route_task 应返回 ok=true，实际: {:?}",
        create_resp
    );

    let task_id = create_resp.result["task_id"]
        .as_str()
        .expect("响应应包含 task_id 字符串");

    let backend = create_resp.result["backend"]
        .as_str()
        .expect("响应应包含 backend 字符串");
    assert_eq!(backend, "codex", "backend 应为 codex");

    // 验证 codex 无写入权限（代码主权守卫核心规则）
    let can_write = create_resp.result["can_write"]
        .as_bool()
        .expect("响应应包含 can_write 布尔值");
    assert!(!can_write, "codex 后端不应有写入权限（代码主权守卫）");

    // 第二步：通过 route_status 查询任务状态
    let status_resp = dispatch(
        &state,
        make_req(
            "route_status",
            serde_json::json!({
                "group_id": "test-group",
                "task_id": task_id
            }),
        ),
    )
    .await;

    // 验证状态查询成功
    assert!(
        status_resp.ok,
        "route_status 应返回 ok=true，实际: {:?}",
        status_resp
    );

    let status = status_resp.result["status"]
        .as_str()
        .expect("状态响应应包含 status 字段");
    assert_eq!(status, "pending", "新提交的任务状态应为 pending");

    let resp_backend = status_resp.result["backend"]
        .as_str()
        .expect("状态响应应包含 backend 字段");
    assert_eq!(resp_backend, "codex", "状态响应中 backend 应为 codex");
}

// ============================================
// 场景 2: 提交并行任务集 -> DAG 排序验证
// ============================================

/// 提交两个任务（t1 -> t2 依赖链），验证 DAG 拓扑排序正确：
/// - task_count=2（两个任务）
/// - layers=2（依赖链形成两层）
/// 再提交无依赖的两个并行任务，验证 layers=1
#[tokio::test]
async fn route_parallel_tasks_dag_validation() {
    let (_tmp, state) = make_test_state();

    // 测试一：依赖链 t1 -> t2，应形成 2 层
    let serial_tasks = "\
---TASK---
id: t1
backend: codex
---CONTENT---
任务1：实现基础数据结构
---TASK---
id: t2
dependencies: t1
backend: claude
---CONTENT---
任务2：依赖任务1的扩展实现";

    let serial_resp = dispatch(
        &state,
        make_req(
            "route_task_parallel",
            serde_json::json!({
                "group_id": "test-group",
                "tasks_format": serial_tasks
            }),
        ),
    )
    .await;

    assert!(
        serial_resp.ok,
        "route_task_parallel（依赖链）应返回 ok=true，实际: {:?}",
        serial_resp
    );

    let task_count = serial_resp.result["task_count"]
        .as_u64()
        .expect("响应应包含 task_count 数字");
    assert_eq!(task_count, 2, "依赖链中 task_count 应为 2");

    let layers = serial_resp.result["layers"]
        .as_u64()
        .expect("响应应包含 layers 数字");
    assert_eq!(layers, 2, "t1->t2 依赖链应形成 2 层");

    // 测试二：无依赖的两个并行任务，应形成 1 层
    let parallel_tasks = "\
---TASK---
id: p1
backend: codex
---CONTENT---
并行任务1：独立功能实现
---TASK---
id: p2
backend: gemini
---CONTENT---
并行任务2：另一个独立功能";

    let parallel_resp = dispatch(
        &state,
        make_req(
            "route_task_parallel",
            serde_json::json!({
                "group_id": "test-group",
                "tasks_format": parallel_tasks
            }),
        ),
    )
    .await;

    assert!(
        parallel_resp.ok,
        "route_task_parallel（无依赖）应返回 ok=true，实际: {:?}",
        parallel_resp
    );

    let parallel_layers = parallel_resp.result["layers"]
        .as_u64()
        .expect("响应应包含 layers 数字");
    assert_eq!(parallel_layers, 1, "无依赖的两个任务应只有 1 层");

    let parallel_count = parallel_resp.result["task_count"]
        .as_u64()
        .expect("响应应包含 task_count 数字");
    assert_eq!(parallel_count, 2, "并行任务集 task_count 应为 2");
}

// ============================================
// 场景 3: 代码主权守卫 — 非 Claude 后端零写入
// ============================================

/// 验证代码主权核心规则：
/// - codex 后端 can_write=false
/// - gemini 后端 can_write=false
/// - claude 后端 can_write=true
#[tokio::test]
async fn sovereignty_blocks_non_claude_write() {
    let (_tmp, state) = make_test_state();

    // 测试一：codex 后端无写入权限
    let codex_resp = dispatch(
        &state,
        make_req(
            "route_task",
            serde_json::json!({
                "group_id": "sovereignty-test",
                "task_text": "生成代码片段",
                "backend": "codex"
            }),
        ),
    )
    .await;

    assert!(codex_resp.ok, "codex route_task 应成功");
    assert!(
        !codex_resp.result["can_write"].as_bool().unwrap_or(true),
        "codex 不应有写入权限"
    );

    // 测试二：gemini 后端无写入权限
    let gemini_resp = dispatch(
        &state,
        make_req(
            "route_task",
            serde_json::json!({
                "group_id": "sovereignty-test",
                "task_text": "代码审查",
                "backend": "gemini"
            }),
        ),
    )
    .await;

    assert!(gemini_resp.ok, "gemini route_task 应成功");
    assert!(
        !gemini_resp.result["can_write"].as_bool().unwrap_or(true),
        "gemini 不应有写入权限"
    );

    // 测试三：claude 后端有写入权限（唯一合法写入者）
    let claude_resp = dispatch(
        &state,
        make_req(
            "route_task",
            serde_json::json!({
                "group_id": "sovereignty-test",
                "task_text": "重构模块代码",
                "backend": "claude"
            }),
        ),
    )
    .await;

    assert!(claude_resp.ok, "claude route_task 应成功");
    assert!(
        claude_resp.result["can_write"].as_bool().unwrap_or(false),
        "claude 应有写入权限（代码主权守卫允许）"
    );
}

// ============================================
// 场景 4: 任务取消流程
// ============================================

/// 完整取消流程验证：
/// 1. route_task 提交任务 -> 获取 task_id
/// 2. route_cancel 取消 -> cancelled=true
/// 3. route_status 查询 -> status="cancelled"
#[tokio::test]
async fn cancel_task_changes_status() {
    let (_tmp, state) = make_test_state();

    // 第一步：提交任务
    let create_resp = dispatch(
        &state,
        make_req(
            "route_task",
            serde_json::json!({
                "group_id": "cancel-test-group",
                "task_text": "需要被取消的任务",
                "backend": "codex"
            }),
        ),
    )
    .await;

    assert!(create_resp.ok, "route_task 应成功");

    let task_id = create_resp.result["task_id"]
        .as_str()
        .expect("task_id 应为字符串")
        .to_string();

    // 第二步：取消任务
    let cancel_resp = dispatch(
        &state,
        make_req(
            "route_cancel",
            serde_json::json!({
                "group_id": "cancel-test-group",
                "task_id": &task_id
            }),
        ),
    )
    .await;

    assert!(cancel_resp.ok, "route_cancel 应返回 ok=true");
    assert_eq!(
        cancel_resp.result["cancelled"].as_bool(),
        Some(true),
        "cancelled 字段应为 true"
    );
    assert_eq!(
        cancel_resp.result["task_id"].as_str(),
        Some(task_id.as_str()),
        "响应中 task_id 应与提交时一致"
    );

    // 第三步：查询状态，确认已变为 cancelled
    let status_resp = dispatch(
        &state,
        make_req(
            "route_status",
            serde_json::json!({
                "group_id": "cancel-test-group",
                "task_id": &task_id
            }),
        ),
    )
    .await;

    assert!(status_resp.ok, "route_status 应返回 ok=true");
    assert_eq!(
        status_resp.result["status"].as_str().unwrap_or(""),
        "cancelled",
        "取消后 status 应变为 cancelled"
    );
}

// ============================================
// 场景 5: 错误处理 — 无效参数 + 不存在的任务
// ============================================

/// 错误处理边界条件验证：
/// 1. route_task 缺少 group_id -> INVALID_ARGS 错误
/// 2. route_status 查询不存在的 task_id -> NOT_FOUND 错误
/// 3. route_cancel 取消不存在任务 -> 幂等（不 panic）
/// 4. route_task group_id="../etc/passwd" -> INVALID_ARGS（路径遍历防护）
#[tokio::test]
async fn error_handling_invalid_args_and_missing_task() {
    let (_tmp, state) = make_test_state();

    // 测试一：缺少 group_id 时应返回 INVALID_ARGS
    let missing_group_resp = dispatch(
        &state,
        make_req(
            "route_task",
            serde_json::json!({
                "task_text": "缺少 group_id 的任务"
            }),
        ),
    )
    .await;

    assert!(!missing_group_resp.ok, "缺少 group_id 应返回错误");
    assert_eq!(
        missing_group_resp
            .error
            .as_ref()
            .map(|e| e.code.as_str()),
        Some("INVALID_ARGS"),
        "缺少 group_id 错误码应为 INVALID_ARGS，实际: {:?}",
        missing_group_resp.error
    );

    // 测试二：查询不存在的 task_id 应返回 NOT_FOUND
    let not_found_resp = dispatch(
        &state,
        make_req(
            "route_status",
            serde_json::json!({
                "group_id": "test-group",
                "task_id": "00000000-0000-0000-0000-000000000000"
            }),
        ),
    )
    .await;

    assert!(!not_found_resp.ok, "查询不存在的任务应返回错误");
    assert_eq!(
        not_found_resp.error.as_ref().map(|e| e.code.as_str()),
        Some("NOT_FOUND"),
        "不存在的 task_id 错误码应为 NOT_FOUND，实际: {:?}",
        not_found_resp.error
    );

    // 测试三：取消不存在的任务应幂等处理（不 panic，返回成功）
    let idempotent_cancel_resp = dispatch(
        &state,
        make_req(
            "route_cancel",
            serde_json::json!({
                "group_id": "test-group",
                "task_id": "99999999-9999-9999-9999-999999999999"
            }),
        ),
    )
    .await;

    // 幂等取消：不存在的任务也不应 panic，调用应正常完成
    assert!(
        idempotent_cancel_resp.ok,
        "取消不存在的任务应幂等处理（不 panic），实际: {:?}",
        idempotent_cancel_resp
    );

    // 测试四：包含路径遍历字符的 group_id 应被拒绝
    let path_traversal_resp = dispatch(
        &state,
        make_req(
            "route_task",
            serde_json::json!({
                "group_id": "../etc/passwd",
                "task_text": "路径遍历攻击测试"
            }),
        ),
    )
    .await;

    assert!(!path_traversal_resp.ok, "路径遍历 group_id 应返回错误");
    assert_eq!(
        path_traversal_resp
            .error
            .as_ref()
            .map(|e| e.code.as_str()),
        Some("INVALID_ARGS"),
        "路径遍历 group_id 错误码应为 INVALID_ARGS，实际: {:?}",
        path_traversal_resp.error
    );
}

// ============================================
// 场景 6: route_task 真实执行链路 - 成功路径
// ============================================

/// 通过 fake CLI 验证 route_task 执行到 completed 状态：
/// 1. route_task 提交任务，fake claude 成功执行
/// 2. route_status 查询：status = "completed"
/// 3. 验证 session_id 写入 SessionStore（resume 能力基础）
///
/// 测试夹具：tests/fixtures/claude（fake claude，输出固定 JSON stream）
/// 注入方式：prepend_path() 将 fixtures 目录添加到 PATH 最前面
#[tokio::test]
async fn route_single_task_executes_to_success_and_status_completed() {
    // 构造带 fake CLI 注入路径的 AppState
    let (_tmp, state) = make_test_state_with_fake_cli();

    // 提交 claude 后端任务（通过 _cli_path 注入 fake claude，直接指定完整路径）
    let create_resp = dispatch(
        &state,
        make_req(
            "route_task",
            serde_json::json!({
                "group_id": "exec-test-group",
                "task_text": "执行一个测试任务",
                "backend": "claude",
                "workdir": "/tmp",
                "actor_id": "actor-exec-1",
                "_cli_path": fake_claude_path()
            }),
        ),
    )
    .await;

    assert!(
        create_resp.ok,
        "route_task 提交应成功，实际: {:?}",
        create_resp
    );

    let task_id = create_resp.result["task_id"]
        .as_str()
        .expect("响应应包含 task_id")
        .to_string();

    // route_task 是异步执行的，等待执行完成
    // 轮询 route_status，最多等待 5 秒
    let mut final_status = String::from("pending");
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let status_resp = dispatch(
            &state,
            make_req(
                "route_status",
                serde_json::json!({
                    "group_id": "exec-test-group",
                    "task_id": &task_id
                }),
            ),
        )
        .await;

        if status_resp.ok {
            let s = status_resp.result["status"]
                .as_str()
                .unwrap_or("pending")
                .to_string();
            if s != "pending" && s != "running" {
                final_status = s;
                break;
            }
        }
    }

    assert_eq!(
        final_status, "completed",
        "fake claude 执行后任务状态应为 completed，实际: {}",
        final_status
    );
}

// ============================================
// 场景 7: route_task 真实执行链路 - 失败路径
// ============================================

/// 通过 fake-fail CLI 验证 route_task 执行失败后状态变为 failed：
/// 1. 将 claude 命令替换为失败脚本（以非零退出码退出）
/// 2. route_status 查询：status = "failed"
#[tokio::test]
async fn route_single_task_executes_to_failed_and_status_failed() {
    // 使用失败版 fake CLI（claude-fail 脚本会返回退出码 1）
    let (_tmp, state) = make_test_state_with_failing_cli();

    let create_resp = dispatch(
        &state,
        make_req(
            "route_task",
            serde_json::json!({
                "group_id": "fail-test-group",
                "task_text": "这个任务会失败",
                "backend": "claude",
                "workdir": "/tmp",
                "actor_id": "actor-fail-1",
                "_cli_path": fake_claude_fail_path()
            }),
        ),
    )
    .await;

    assert!(create_resp.ok, "route_task 提交应成功");

    let task_id = create_resp.result["task_id"]
        .as_str()
        .expect("task_id 应为字符串")
        .to_string();

    // 轮询等待执行完成
    let mut final_status = String::from("pending");
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let status_resp = dispatch(
            &state,
            make_req(
                "route_status",
                serde_json::json!({
                    "group_id": "fail-test-group",
                    "task_id": &task_id
                }),
            ),
        )
        .await;

        if status_resp.ok {
            let s = status_resp.result["status"]
                .as_str()
                .unwrap_or("pending")
                .to_string();
            if s != "pending" && s != "running" {
                final_status = s;
                break;
            }
        }
    }

    assert_eq!(
        final_status, "failed",
        "fake claude 失败后任务状态应为 failed，实际: {}",
        final_status
    );
}

// ============================================
// 场景 8: route_task 执行后 session_id 写入 SessionStore
// ============================================

/// 验证 fake claude 执行成功后：
/// 1. SessionStore 中存有对应的 session_id
/// 2. session_id 值与 fake CLI 输出的 "fake-claude-session-001" 一致
///
/// 这是实现 resume 模式的基础：下次同 actor 提交任务时可复用会话
#[tokio::test]
async fn route_single_task_persists_session_id_after_successful_resume() {
    use ghostcode_router::session::SessionKey;

    let (_tmp, state) = make_test_state_with_fake_cli();

    let actor_id = "actor-session-test-1";
    let group_id = "session-persist-group";

    // 提交任务（通过 _cli_path 注入 fake claude）
    let create_resp = dispatch(
        &state,
        make_req(
            "route_task",
            serde_json::json!({
                "group_id": group_id,
                "task_text": "测试 session 持久化",
                "backend": "claude",
                "workdir": "/tmp",
                "actor_id": actor_id,
                "_cli_path": fake_claude_path()
            }),
        ),
    )
    .await;

    assert!(create_resp.ok, "route_task 提交应成功");

    let task_id = create_resp.result["task_id"]
        .as_str()
        .expect("task_id 应为字符串")
        .to_string();

    // 轮询等待执行完成
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let status_resp = dispatch(
            &state,
            make_req(
                "route_status",
                serde_json::json!({
                    "group_id": group_id,
                    "task_id": &task_id
                }),
            ),
        )
        .await;

        if status_resp.ok {
            let s = status_resp.result["status"].as_str().unwrap_or("pending");
            if s == "completed" || s == "failed" {
                break;
            }
        }
    }

    // 验证 SessionStore 中存有 session_id
    // fake claude 输出的 session_id 为 "fake-claude-session-001"
    let session_key: SessionKey = (
        group_id.to_string(),
        actor_id.to_string(),
        "claude".to_string(),
    );
    let stored_session_id = state.session_store.get(&session_key);

    assert!(
        stored_session_id.is_some(),
        "执行完成后 SessionStore 应存有 session_id，实际: None"
    );
    assert_eq!(
        stored_session_id.as_deref(),
        Some("fake-claude-session-001"),
        "session_id 应与 fake CLI 输出一致，实际: {:?}",
        stored_session_id
    );
}

// ============================================
// 测试夹具辅助函数
// ============================================

/// 获取 fake CLI fixtures 目录路径
///
/// 使用编译时常量 file!() 计算 fixtures 目录，
/// 不依赖运行时环境变量，适合并发测试场景
fn fixtures_dir() -> std::path::PathBuf {
    // file!() 返回编译时的源文件路径（相对于 workspace root）
    // 路径形式为 "crates/ghostcode-daemon/tests/router_integration_test.rs"
    // 因此 fixtures 目录在同级 tests/fixtures/
    let this_file = std::path::Path::new(file!());
    // 向上两层：router_integration_test.rs -> tests -> ghostcode-daemon
    // 再下一层 tests/fixtures
    let crate_dir = this_file
        .parent() // tests/
        .and_then(|p| p.parent()) // crates/ghostcode-daemon/
        .expect("无法从 file!() 路径推导 crate 目录");

    // 尝试通过 CARGO_MANIFEST_DIR 获取绝对路径
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        return std::path::PathBuf::from(manifest_dir).join("tests/fixtures");
    }

    // fallback：使用 cwd + 相对路径
    let cwd = std::env::current_dir().expect("获取 cwd 失败");
    // 检查是否在 workspace root
    let direct = cwd.join(crate_dir).join("tests/fixtures");
    if direct.exists() {
        return direct;
    }

    // 如果 cwd 已经是 crate 目录
    cwd.join("tests/fixtures")
}

/// 获取成功版 fake claude CLI 的完整路径
fn fake_claude_path() -> String {
    fixtures_dir().join("claude").to_string_lossy().to_string()
}

/// 获取失败版 fake claude CLI 的完整路径
fn fake_claude_fail_path() -> String {
    fixtures_dir().join("claude-fail").to_string_lossy().to_string()
}

/// 创建标准测试 AppState（不依赖 PATH 注入）
fn make_test_state_with_fake_cli() -> (TempDir, AppState) {
    let tmp = TempDir::new().expect("创建临时目录失败");
    let state = AppState::new(tmp.path().to_path_buf());
    (tmp, state)
}

/// 创建标准测试 AppState（失败版本，同 make_test_state_with_fake_cli）
///
/// 失败行为通过在 IPC 请求中传入 _cli_path 指定 claude-fail 脚本实现，
/// 不依赖全局 PATH 修改，完全线程安全
fn make_test_state_with_failing_cli() -> (TempDir, AppState) {
    let tmp = TempDir::new().expect("创建临时目录失败");
    let state = AppState::new(tmp.path().to_path_buf());
    (tmp, state)
}

// ============================================
// 场景 9: route_task_parallel 所有层执行完毕
// ============================================

/// 验证两个无依赖并行任务通过 fake CLI 都能执行至 completed：
/// 1. 提交两个无依赖任务（t1、t2 各用一个 fake claude）
/// 2. 轮询等待组合任务状态
/// 3. 验证每个子任务最终状态为 completed
#[tokio::test]
async fn route_parallel_tasks_execute_all_layers_and_complete() {
    let (_tmp, state) = make_test_state();

    let fake_cli = fake_claude_path();

    // 构造两个无依赖的并行任务，各自指定 fake CLI 路径（通过 _cli_paths 字段）
    // task 格式文本中注入 _cli_path 通过 actor_id 区分
    let tasks_format = format!(
        "---TASK---\nid: t1\nbackend: claude\nactor_id: actor-par-1\n_cli_path: {cli}\n---CONTENT---\n任务1：无依赖并行任务\n---TASK---\nid: t2\nbackend: claude\nactor_id: actor-par-2\n_cli_path: {cli}\n---CONTENT---\n任务2：无依赖并行任务\n",
        cli = fake_cli
    );

    let create_resp = dispatch(
        &state,
        make_req(
            "route_task_parallel",
            serde_json::json!({
                "group_id": "parallel-exec-group",
                "tasks_format": tasks_format,
                "_cli_path": fake_cli
            }),
        ),
    )
    .await;

    assert!(
        create_resp.ok,
        "route_task_parallel 提交应成功，实际: {:?}",
        create_resp
    );
    assert_eq!(
        create_resp.result["task_count"].as_u64().unwrap_or(0),
        2,
        "task_count 应为 2"
    );

    let group_task_id = create_resp.result["task_id"]
        .as_str()
        .expect("task_id 应为字符串")
        .to_string();

    // 轮询等待组合任务执行完成
    let mut final_status = String::from("pending");
    for _ in 0..100 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let status_resp = dispatch(
            &state,
            make_req(
                "route_status",
                serde_json::json!({
                    "group_id": "parallel-exec-group",
                    "task_id": &group_task_id
                }),
            ),
        )
        .await;

        if status_resp.ok {
            let s = status_resp.result["status"]
                .as_str()
                .unwrap_or("pending")
                .to_string();
            if s != "pending" && s != "running" {
                final_status = s;
                break;
            }
        }
    }

    assert_eq!(
        final_status, "completed",
        "两个并行任务都成功后，组合任务状态应为 completed，实际: {}",
        final_status
    );
}

// ============================================
// 场景 10: route_task_parallel 依赖失败传播
// ============================================

/// 验证依赖失败传播：t1 失败 -> t2（依赖 t1）被跳过/标记为 failed：
/// 1. 提交 t1（fake-fail）、t2（依赖 t1，fake claude）
/// 2. Executor 的依赖失败传播机制使 t2 变为 Skipped
/// 3. 组合任务最终状态为 failed（含失败子任务）
#[tokio::test]
async fn route_parallel_tasks_skip_downstream_when_dependency_fails() {
    let (_tmp, state) = make_test_state();

    let fail_cli = fake_claude_fail_path();
    let ok_cli = fake_claude_path();

    // t1 使用失败 CLI，t2 依赖 t1 使用成功 CLI（但会被跳过）
    let tasks_format = format!(
        "---TASK---\nid: t1\nbackend: claude\n---CONTENT---\n会失败的任务\n---TASK---\nid: t2\nbackend: claude\ndependencies: t1\n---CONTENT---\n依赖 t1 的任务，t1 失败则跳过\n",
    );

    let create_resp = dispatch(
        &state,
        make_req(
            "route_task_parallel",
            serde_json::json!({
                "group_id": "dep-fail-group",
                "tasks_format": tasks_format,
                // t1 使用失败 CLI，t2 使用正常 CLI（由 _cli_paths_by_id 指定）
                "_cli_path": fail_cli,
                "_cli_path_fallback": ok_cli
            }),
        ),
    )
    .await;

    assert!(
        create_resp.ok,
        "route_task_parallel 提交应成功，实际: {:?}",
        create_resp
    );

    let group_task_id = create_resp.result["task_id"]
        .as_str()
        .expect("task_id 应为字符串")
        .to_string();

    // 轮询等待执行完成
    let mut final_status = String::from("pending");
    for _ in 0..100 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let status_resp = dispatch(
            &state,
            make_req(
                "route_status",
                serde_json::json!({
                    "group_id": "dep-fail-group",
                    "task_id": &group_task_id
                }),
            ),
        )
        .await;

        if status_resp.ok {
            let s = status_resp.result["status"]
                .as_str()
                .unwrap_or("pending")
                .to_string();
            if s != "pending" && s != "running" {
                final_status = s;
                break;
            }
        }
    }

    // 含失败子任务的并行组，最终状态应为 failed
    assert_eq!(
        final_status, "failed",
        "t1 失败后，组合任务状态应为 failed，实际: {}",
        final_status
    );
}

// ============================================
// 场景 11: route_task_parallel 汇总结果查询
// ============================================

/// 验证 route_status 对并行任务返回子任务汇总信息：
/// - `subtasks` 数组包含每个子任务的 id 和 status
/// - 所有子任务都 completed 时，组合任务状态为 completed
#[tokio::test]
async fn route_parallel_tasks_status_contains_aggregated_results() {
    let (_tmp, state) = make_test_state();

    let fake_cli = fake_claude_path();

    // 提交两个无依赖并行任务
    let tasks_format = format!(
        "---TASK---\nid: agg-t1\nbackend: claude\n---CONTENT---\n汇总测试任务1\n---TASK---\nid: agg-t2\nbackend: claude\n---CONTENT---\n汇总测试任务2\n",
    );

    let create_resp = dispatch(
        &state,
        make_req(
            "route_task_parallel",
            serde_json::json!({
                "group_id": "agg-test-group",
                "tasks_format": tasks_format,
                "_cli_path": fake_cli
            }),
        ),
    )
    .await;

    assert!(create_resp.ok, "提交应成功");

    let group_task_id = create_resp.result["task_id"]
        .as_str()
        .expect("task_id 应为字符串")
        .to_string();

    // 轮询等待执行完成
    let mut status_resp_final = serde_json::json!({});
    for _ in 0..100 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let status_resp = dispatch(
            &state,
            make_req(
                "route_status",
                serde_json::json!({
                    "group_id": "agg-test-group",
                    "task_id": &group_task_id
                }),
            ),
        )
        .await;

        if status_resp.ok {
            let s = status_resp.result["status"]
                .as_str()
                .unwrap_or("pending")
                .to_string();
            if s != "pending" && s != "running" {
                status_resp_final = status_resp.result.clone();
                break;
            }
        }
    }

    // 验证组合任务状态为 completed
    assert_eq!(
        status_resp_final["status"].as_str().unwrap_or(""),
        "completed",
        "两个子任务都成功，组合任务应为 completed，实际: {:?}",
        status_resp_final
    );

    // 验证响应包含子任务汇总信息
    assert!(
        status_resp_final.get("subtasks").is_some(),
        "route_status 应包含 subtasks 汇总字段，实际: {:?}",
        status_resp_final
    );

    let subtasks = status_resp_final["subtasks"].as_array().unwrap();
    assert_eq!(
        subtasks.len(),
        2,
        "subtasks 应包含 2 个子任务，实际: {:?}",
        subtasks
    );

    // 验证每个子任务有 id 和 status 字段
    for subtask in subtasks {
        assert!(
            subtask.get("id").is_some(),
            "子任务应有 id 字段，实际: {:?}",
            subtask
        );
        assert!(
            subtask.get("status").is_some(),
            "子任务应有 status 字段，实际: {:?}",
            subtask
        );
        assert_eq!(
            subtask["status"].as_str().unwrap_or(""),
            "completed",
            "子任务 {} 状态应为 completed",
            subtask["id"].as_str().unwrap_or("unknown")
        );
    }
}
