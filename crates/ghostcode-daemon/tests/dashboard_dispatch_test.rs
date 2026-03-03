//! Dashboard Dispatch 集成测试
//!
//! 验证 dashboard_snapshot / dashboard_timeline / dashboard_agents op
//! 通过 dispatch 路由到正确的 handler 函数
//!
//! @author Atlas.oi
//! @date 2026-03-03

use ghostcode_daemon::dispatch::dispatch;
use ghostcode_daemon::server::AppState;
use ghostcode_types::ipc::DaemonRequest;
use serde_json::json;

/// 构造 DaemonRequest 辅助函数
fn make_req(op: &str, args: serde_json::Value) -> DaemonRequest {
    DaemonRequest {
        v: 1,
        op: op.to_string(),
        args,
    }
}

#[tokio::test]
async fn dashboard_snapshot_returns_ok() {
    let state = AppState::default();
    let req = make_req(
        "dashboard_snapshot",
        json!({ "group_id": "test-group" }),
    );
    let resp = dispatch(&state, req).await;
    assert!(
        resp.ok,
        "dashboard_snapshot 应返回 ok=true，错误: {:?}",
        resp.error
    );
}

#[tokio::test]
async fn dashboard_timeline_returns_ok() {
    let state = AppState::default();
    let req = make_req(
        "dashboard_timeline",
        json!({ "group_id": "test-group", "page_size": 10 }),
    );
    let resp = dispatch(&state, req).await;
    assert!(
        resp.ok,
        "dashboard_timeline 应返回 ok=true，错误: {:?}",
        resp.error
    );
}

#[tokio::test]
async fn dashboard_agents_returns_ok() {
    let state = AppState::default();
    let req = make_req(
        "dashboard_agents",
        json!({ "group_id": "test-group" }),
    );
    let resp = dispatch(&state, req).await;
    assert!(
        resp.ok,
        "dashboard_agents 应返回 ok=true，错误: {:?}",
        resp.error
    );
}

#[tokio::test]
async fn missing_group_id_returns_error() {
    let state = AppState::default();
    let req = make_req("dashboard_snapshot", json!({}));
    let resp = dispatch(&state, req).await;
    assert!(!resp.ok, "缺少 group_id 应返回错误");
}

#[tokio::test]
async fn existing_ops_unaffected() {
    let state = AppState::default();
    let req = make_req("ping", json!({}));
    let resp = dispatch(&state, req).await;
    assert!(resp.ok, "ping op 应仍然正常工作");
}
