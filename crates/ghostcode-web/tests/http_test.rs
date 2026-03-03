//! ghostcode-web HTTP 接口测试
//!
//! 验证 REST 端点返回正确格式的响应
//!
//! @author Atlas.oi
//! @date 2026-03-03

use axum::body::Body;
use axum::http::{Request, StatusCode};
use ghostcode_web::server::create_router;
use ghostcode_web::state::WebState;
use tower::ServiceExt;
use tempfile::TempDir;

fn make_state() -> (WebState, TempDir) {
    let dir = TempDir::new().unwrap();
    let state = WebState::new(dir.path().to_path_buf());
    (state, dir)
}

#[tokio::test]
async fn health_check_returns_200() {
    let (state, _dir) = make_state();
    let app = create_router(state);
    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn dashboard_snapshot_unknown_group_returns_ok() {
    let (state, _dir) = make_state();
    let app = create_router(state);
    let req = Request::builder()
        .uri("/api/groups/unknown-group/dashboard")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    // 账本不存在时应返回 200（空快照），不应 500
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn timeline_unknown_group_returns_ok() {
    let (state, _dir) = make_state();
    let app = create_router(state);
    let req = Request::builder()
        .uri("/api/groups/unknown-group/timeline")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn agents_unknown_group_returns_ok() {
    let (state, _dir) = make_state();
    let app = create_router(state);
    let req = Request::builder()
        .uri("/api/groups/unknown-group/agents")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
