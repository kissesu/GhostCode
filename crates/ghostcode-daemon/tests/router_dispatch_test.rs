//! router_dispatch_test.rs
//!
//! T30 路由 Dispatch 集成测试
//! 通过直接调用 dispatch() 函数验证 5 个新路由 op 的行为
//! 不需要启动真实 Unix Socket Server
//!
//! @author Atlas.oi
//! @date 2026-03-02

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
// T1: route_task 返回 task_id
// ============================================

/// 验证 route_task op 返回 ok 响应并包含 task_id
#[tokio::test]
async fn route_task_returns_task_id() {
    let state = AppState::default();

    let req = make_req(
        "route_task",
        serde_json::json!({
            "group_id": "test-group",
            "task_text": "实现一个排序算法",
            "backend": "claude"
        }),
    );

    let resp = dispatch(&state, req).await;

    // 断言响应成功
    assert!(resp.ok, "route_task 应返回 ok=true，实际: {:?}", resp);
    // 断言包含 task_id 字段
    assert!(
        resp.result.get("task_id").is_some(),
        "响应应包含 task_id 字段，实际 result: {:?}",
        resp.result
    );
    // 断言包含 backend 字段
    assert!(
        resp.result.get("backend").is_some(),
        "响应应包含 backend 字段，实际 result: {:?}",
        resp.result
    );
}

// ============================================
// T2: route_status 返回任务状态
// ============================================

/// 先 route_task 获取 task_id，再 route_status 查询状态
#[tokio::test]
async fn route_status_returns_state() {
    let state = AppState::default();

    // 第一步：创建任务
    let create_req = make_req(
        "route_task",
        serde_json::json!({
            "group_id": "test-group",
            "task_text": "生成单元测试",
            "backend": "claude"
        }),
    );
    let create_resp = dispatch(&state, create_req).await;
    assert!(create_resp.ok, "route_task 应成功");

    let task_id = create_resp.result["task_id"]
        .as_str()
        .expect("task_id 应为字符串");

    // 第二步：查询状态
    let status_req = make_req(
        "route_status",
        serde_json::json!({
            "group_id": "test-group",
            "task_id": task_id
        }),
    );
    let status_resp = dispatch(&state, status_req).await;

    assert!(status_resp.ok, "route_status 应返回 ok=true");
    assert!(
        status_resp.result.get("task_id").is_some(),
        "状态响应应包含 task_id"
    );
    assert!(
        status_resp.result.get("status").is_some(),
        "状态响应应包含 status 字段"
    );
}

// ============================================
// T3: route_cancel 标记任务为 cancelled
// ============================================

/// route_cancel 后再 route_status 应看到 status=cancelled
#[tokio::test]
async fn route_cancel_marks_cancelled() {
    let state = AppState::default();

    // 创建任务
    let create_resp = dispatch(
        &state,
        make_req(
            "route_task",
            serde_json::json!({
                "group_id": "grp1",
                "task_text": "重构数据库层",
            }),
        ),
    )
    .await;
    assert!(create_resp.ok);

    let task_id = create_resp.result["task_id"]
        .as_str()
        .expect("task_id 应为字符串");

    // 取消任务
    let cancel_resp = dispatch(
        &state,
        make_req(
            "route_cancel",
            serde_json::json!({
                "group_id": "grp1",
                "task_id": task_id
            }),
        ),
    )
    .await;
    assert!(cancel_resp.ok, "route_cancel 应返回 ok=true");
    assert_eq!(
        cancel_resp.result["cancelled"], true,
        "cancelled 字段应为 true"
    );

    // 验证状态已更新为 cancelled
    let status_resp = dispatch(
        &state,
        make_req(
            "route_status",
            serde_json::json!({
                "group_id": "grp1",
                "task_id": task_id
            }),
        ),
    )
    .await;
    assert!(status_resp.ok);
    assert_eq!(
        status_resp.result["status"].as_str().unwrap_or(""),
        "cancelled",
        "status 应为 cancelled"
    );
}

// ============================================
// T4: session_list 返回空列表
// ============================================

/// session_list 当前返回空 sessions 列表（SessionStore 集成在后续迭代完成）
#[tokio::test]
async fn session_list_returns_empty() {
    let state = AppState::default();

    let resp = dispatch(
        &state,
        make_req(
            "session_list",
            serde_json::json!({
                "group_id": "test-group"
            }),
        ),
    )
    .await;

    assert!(resp.ok, "session_list 应返回 ok=true");
    assert!(
        resp.result["sessions"].is_array(),
        "sessions 应为数组"
    );
    assert_eq!(
        resp.result["sessions"].as_array().unwrap().len(),
        0,
        "sessions 当前应为空列表"
    );
}

// ============================================
// T5: route_task_parallel 接受 ---TASK--- 格式
// ============================================

/// route_task_parallel 接受标准 ---TASK---/---CONTENT--- 格式文本
#[tokio::test]
async fn route_task_parallel_accepts_format() {
    let state = AppState::default();

    // 构造合法的并行任务格式文本
    let tasks_format = "\
---TASK---
id: task1
backend: codex
---CONTENT---
实现登录接口
---TASK---
id: task2
backend: codex
dependencies: task1
---CONTENT---
实现注册接口
";

    let resp = dispatch(
        &state,
        make_req(
            "route_task_parallel",
            serde_json::json!({
                "group_id": "test-group",
                "tasks_format": tasks_format
            }),
        ),
    )
    .await;

    assert!(resp.ok, "route_task_parallel 应返回 ok=true，错误: {:?}", resp);
    assert!(
        resp.result.get("task_id").is_some(),
        "响应应包含 task_id"
    );
    assert_eq!(
        resp.result["task_count"].as_u64().unwrap_or(0),
        2,
        "task_count 应为 2"
    );
}

// ============================================
// T6: 非法 group_id 被拒绝
// ============================================

/// 包含 ../ 的 group_id 应返回 INVALID_ARGS 错误（路径遍历防护）
#[tokio::test]
async fn invalid_group_id_rejected() {
    let state = AppState::default();

    let resp = dispatch(
        &state,
        make_req(
            "route_task",
            serde_json::json!({
                "group_id": "../etc/passwd",
                "task_text": "恶意请求"
            }),
        ),
    )
    .await;

    assert!(!resp.ok, "非法 group_id 应返回错误");
    let error_code = resp.error.as_ref().map(|e| e.code.as_str());
    assert_eq!(
        error_code,
        Some("INVALID_ARGS"),
        "错误码应为 INVALID_ARGS，实际: {:?}",
        error_code
    );
}

// ============================================
// T7: 已有 op 不受影响（ping 正常工作）
// ============================================

/// 验证新增 op 不影响已有功能，ping 应正常返回 pong
#[tokio::test]
async fn existing_ops_still_work() {
    let state = AppState::default();

    let resp = dispatch(
        &state,
        make_req("ping", serde_json::json!({})),
    )
    .await;

    assert!(resp.ok, "ping 应返回 ok=true");
    assert_eq!(
        resp.result["pong"].as_bool(),
        Some(true),
        "ping 应返回 pong=true"
    );
}

// ============================================
// T8: 未知 op 返回 UNKNOWN_OP 错误
// ============================================

/// 未注册的 op 应返回 UNKNOWN_OP 错误码
#[tokio::test]
async fn unknown_op_returns_error() {
    let state = AppState::default();

    let resp = dispatch(
        &state,
        make_req("not_a_real_op_xyz", serde_json::json!({})),
    )
    .await;

    assert!(!resp.ok, "未知 op 应返回错误");
    let error_code = resp.error.as_ref().map(|e| e.code.as_str());
    assert_eq!(
        error_code,
        Some("UNKNOWN_OP"),
        "错误码应为 UNKNOWN_OP"
    );
}
