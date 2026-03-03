//! 验证分发器集成测试（T41）
//!
//! 测试 verification_start/status/cancel 三个 op 的完整行为：
//! - start: 创建新运行，返回 run_id 和初始状态
//! - status: 查询运行状态
//! - cancel: 取消进行中的运行
//! 以及各种错误处理场景
//!
//! @author Atlas.oi
//! @date 2026-03-03

use ghostcode_daemon::dispatch::dispatch;
use ghostcode_daemon::server::AppState;
use ghostcode_types::ipc::DaemonRequest;

// ============================================
// verification_start 测试
// ============================================

/// 正常启动验证运行，期望返回 run_id 和 Running 状态
#[tokio::test]
async fn verification_start_creates_new_run() {
    let state = AppState::default();
    let req = DaemonRequest::new(
        "verification_start",
        serde_json::json!({ "group_id": "g1", "run_id": "r1" }),
    );
    let resp = dispatch(&state, req).await;

    assert!(resp.ok, "verification_start 应返回 ok: true，实际：{:?}", resp.error);
    assert!(resp.error.is_none(), "verification_start 不应有 error");
    // result 中应包含 run_id 字段
    assert_eq!(
        resp.result["run_id"].as_str(),
        Some("r1"),
        "result 中应包含 run_id: r1"
    );
    // status 应为 Running
    assert_eq!(
        resp.result["status"].as_str(),
        Some("Running"),
        "新建运行的状态应为 Running"
    );
}

/// 缺少 group_id 时返回 INVALID_ARGS 错误
#[tokio::test]
async fn verification_start_missing_group_id() {
    let state = AppState::default();
    let req = DaemonRequest::new(
        "verification_start",
        serde_json::json!({ "run_id": "r1" }),
    );
    let resp = dispatch(&state, req).await;

    assert!(!resp.ok, "缺少 group_id 时应返回 ok: false");
    assert_eq!(
        resp.error.as_ref().unwrap().code,
        "INVALID_ARGS",
        "缺少 group_id 时错误码应为 INVALID_ARGS"
    );
}

/// 缺少 run_id 时返回 INVALID_ARGS 错误
#[tokio::test]
async fn verification_start_missing_run_id() {
    let state = AppState::default();
    let req = DaemonRequest::new(
        "verification_start",
        serde_json::json!({ "group_id": "g1" }),
    );
    let resp = dispatch(&state, req).await;

    assert!(!resp.ok, "缺少 run_id 时应返回 ok: false");
    assert_eq!(
        resp.error.as_ref().unwrap().code,
        "INVALID_ARGS",
        "缺少 run_id 时错误码应为 INVALID_ARGS"
    );
}

/// 重复 start 同一 (group_id, run_id) 时返回错误
#[tokio::test]
async fn verification_start_duplicate_run() {
    let state = AppState::default();

    // 第一次 start
    let req1 = DaemonRequest::new(
        "verification_start",
        serde_json::json!({ "group_id": "g1", "run_id": "r1" }),
    );
    let resp1 = dispatch(&state, req1).await;
    assert!(resp1.ok, "第一次 start 应成功");

    // 第二次 start 同一运行，应返回错误
    let req2 = DaemonRequest::new(
        "verification_start",
        serde_json::json!({ "group_id": "g1", "run_id": "r1" }),
    );
    let resp2 = dispatch(&state, req2).await;
    assert!(!resp2.ok, "重复 start 同一运行应返回 ok: false");
    assert_eq!(
        resp2.error.as_ref().unwrap().code,
        "VERIFICATION_ERROR",
        "重复 start 应返回 VERIFICATION_ERROR"
    );
}

// ============================================
// verification_status 测试
// ============================================

/// 先 start 后查询 status，应返回 Running 状态
#[tokio::test]
async fn verification_status_returns_current_state() {
    let state = AppState::default();

    // 先 start
    let req_start = DaemonRequest::new(
        "verification_start",
        serde_json::json!({ "group_id": "g1", "run_id": "r1" }),
    );
    let resp_start = dispatch(&state, req_start).await;
    assert!(resp_start.ok, "start 应成功");

    // 查询 status
    let req_status = DaemonRequest::new(
        "verification_status",
        serde_json::json!({ "group_id": "g1", "run_id": "r1" }),
    );
    let resp_status = dispatch(&state, req_status).await;

    assert!(resp_status.ok, "verification_status 应返回 ok: true，实际：{:?}", resp_status.error);
    assert_eq!(
        resp_status.result["status"].as_str(),
        Some("Running"),
        "Running 状态下查询应返回 Running"
    );
}

/// 查询不存在的运行时返回 NOT_FOUND 错误
#[tokio::test]
async fn verification_status_not_found() {
    let state = AppState::default();
    let req = DaemonRequest::new(
        "verification_status",
        serde_json::json!({ "group_id": "nonexistent", "run_id": "nonexistent" }),
    );
    let resp = dispatch(&state, req).await;

    assert!(!resp.ok, "查询不存在的运行应返回 ok: false");
    assert_eq!(
        resp.error.as_ref().unwrap().code,
        "NOT_FOUND",
        "查询不存在的运行错误码应为 NOT_FOUND"
    );
}

// ============================================
// verification_cancel 测试
// ============================================

/// 先 start 后 cancel，应返回 ok: true
#[tokio::test]
async fn verification_cancel_cancels_running() {
    let state = AppState::default();

    // 先 start
    let req_start = DaemonRequest::new(
        "verification_start",
        serde_json::json!({ "group_id": "g1", "run_id": "r1" }),
    );
    let resp_start = dispatch(&state, req_start).await;
    assert!(resp_start.ok, "start 应成功");

    // cancel
    let req_cancel = DaemonRequest::new(
        "verification_cancel",
        serde_json::json!({ "group_id": "g1", "run_id": "r1" }),
    );
    let resp_cancel = dispatch(&state, req_cancel).await;

    assert!(resp_cancel.ok, "verification_cancel 应返回 ok: true，实际：{:?}", resp_cancel.error);
    assert_eq!(
        resp_cancel.result["cancelled"].as_bool(),
        Some(true),
        "result 中 cancelled 应为 true"
    );
}
