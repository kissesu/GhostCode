//! ghostcode-web HTTP 接口测试
//!
//! 验证 REST 端点返回正确格式的响应
//! 包含 IPC client 集成测试：skills_list 和 skill_promote 端点
//!
//! @author Atlas.oi
//! @date 2026-03-04

use axum::body::Body;
use axum::http::{Request, StatusCode};
use ghostcode_web::server::create_router;
use ghostcode_web::state::WebState;
use tower::ServiceExt;
use tempfile::TempDir;

fn make_state() -> (WebState, TempDir) {
    let dir = TempDir::new().unwrap();
    // 使用不存在的 socket 路径，测试 daemon 不可达的情况
    let state = WebState::new(dir.path().to_path_buf());
    (state, dir)
}

fn make_state_with_socket(socket_path: std::path::PathBuf) -> (WebState, TempDir) {
    let dir = TempDir::new().unwrap();
    let state = WebState::with_socket(dir.path().to_path_buf(), socket_path);
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

// ============================================
// IPC 集成测试：daemon 不可达时返回 502
// ============================================

/// 验证 skills_list 端点在 daemon 不可达时返回 HTTP 502
///
/// 业务逻辑：
/// 1. WebState 配置一个不存在的 socket 路径（daemon 未启动）
/// 2. 请求 GET /api/groups/:id/skills
/// 3. IPC 连接失败，应返回 502 Bad Gateway
#[tokio::test]
async fn skills_list_returns_502_when_daemon_unreachable() {
    // 使用不存在的 socket 路径，确保 daemon 不可达
    let nonexistent_socket = std::path::PathBuf::from("/tmp/ghostcode-nonexistent-test.sock");
    let (state, _dir) = make_state_with_socket(nonexistent_socket);
    let app = create_router(state);

    let req = Request::builder()
        .uri("/api/groups/test-group/skills")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    // daemon 不可达时，应返回 502 Bad Gateway
    assert_eq!(
        resp.status(),
        StatusCode::BAD_GATEWAY,
        "daemon 不可达时 skills_list 应返回 502"
    );
}

/// 验证 skill_promote 端点在 daemon 不可达时返回 HTTP 502
///
/// 业务逻辑：
/// 1. WebState 配置一个不存在的 socket 路径（daemon 未启动）
/// 2. 请求 POST /api/groups/:id/skills/:skill_id/promote
/// 3. IPC 连接失败，应返回 502 Bad Gateway
#[tokio::test]
async fn skill_promote_returns_502_when_daemon_unreachable() {
    // 使用不存在的 socket 路径，确保 daemon 不可达
    let nonexistent_socket = std::path::PathBuf::from("/tmp/ghostcode-nonexistent-promote.sock");
    let (state, _dir) = make_state_with_socket(nonexistent_socket);
    let app = create_router(state);

    let req = Request::builder()
        .uri("/api/groups/test-group/skills/skill-001/promote")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    // daemon 不可达时，应返回 502 Bad Gateway
    assert_eq!(
        resp.status(),
        StatusCode::BAD_GATEWAY,
        "daemon 不可达时 skill_promote 应返回 502"
    );
}
