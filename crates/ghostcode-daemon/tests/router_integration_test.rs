//! @file router_integration_test.rs
//! @description 多模型路由端到端集成测试 - 验证路由引擎完整流程
//! @author Atlas.oi
//! @date 2026-03-03

use ghostcode_daemon::dispatch::dispatch;
use ghostcode_daemon::server::AppState;
use ghostcode_types::ipc::DaemonRequest;

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
    let state = AppState::default();

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
    let state = AppState::default();

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
    let state = AppState::default();

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
    let state = AppState::default();

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
    let state = AppState::default();

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
