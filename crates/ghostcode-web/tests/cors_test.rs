//! CORS 中间件测试
//!
//! 验证 build_cors_layer 函数生成的 CORS 层行为正确
//! 测试覆盖：预检请求、响应头回显、允许方法、允许头、源白名单拒绝
//!
//! @author Atlas.oi
//! @date 2026-03-05

use axum::body::Body;
use axum::http::{Method, Request};
use ghostcode_web::server::{build_cors_layer, create_router};
use ghostcode_web::state::WebState;
use tempfile::TempDir;
use tower::ServiceExt;

/// 测试：OPTIONS 预检请求返回正确 CORS 头
///
/// 发送 OPTIONS 请求并携带 Origin，验证响应包含 access-control-allow-origin
#[tokio::test]
async fn cors_preflight_returns_correct_headers() {
    let dir = TempDir::new().unwrap();
    let state = WebState::new(dir.path().to_path_buf());
    let origins = vec!["http://localhost:5173".to_string()];
    let cors = build_cors_layer(&origins);
    let app = create_router(state).layer(cors);

    let req = Request::builder()
        .method(Method::OPTIONS)
        .uri("/health")
        .header("Origin", "http://localhost:5173")
        .header("Access-Control-Request-Method", "GET")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();

    // 验证 access-control-allow-origin 头存在
    assert!(
        resp.headers().contains_key("access-control-allow-origin"),
        "预检请求应返回 access-control-allow-origin 头"
    );
}

/// 测试：access-control-allow-origin 回显请求源
///
/// GET 请求携带已允许的 Origin，验证响应回显该源
#[tokio::test]
async fn cors_allow_origin_echoes_request_origin() {
    let dir = TempDir::new().unwrap();
    let state = WebState::new(dir.path().to_path_buf());
    let origins = vec!["http://localhost:5173".to_string()];
    let cors = build_cors_layer(&origins);
    let app = create_router(state).layer(cors);

    let req = Request::builder()
        .method(Method::GET)
        .uri("/health")
        .header("Origin", "http://localhost:5173")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();

    // 验证响应头包含请求源
    let allow_origin = resp
        .headers()
        .get("access-control-allow-origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    assert_eq!(
        allow_origin, "http://localhost:5173",
        "响应应回显请求源 http://localhost:5173"
    );
}

/// 测试：access-control-allow-methods 包含 GET, POST, OPTIONS
///
/// OPTIONS 预检请求响应中验证 allow-methods 包含必要方法
#[tokio::test]
async fn cors_allow_methods_contains_required_methods() {
    let dir = TempDir::new().unwrap();
    let state = WebState::new(dir.path().to_path_buf());
    let origins = vec!["http://localhost:5173".to_string()];
    let cors = build_cors_layer(&origins);
    let app = create_router(state).layer(cors);

    let req = Request::builder()
        .method(Method::OPTIONS)
        .uri("/health")
        .header("Origin", "http://localhost:5173")
        .header("Access-Control-Request-Method", "POST")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();

    // 验证 access-control-allow-methods 包含 GET, POST, OPTIONS
    let allow_methods = resp
        .headers()
        .get("access-control-allow-methods")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    assert!(
        allow_methods.contains("GET"),
        "allow-methods 应包含 GET，实际: {}",
        allow_methods
    );
    assert!(
        allow_methods.contains("POST"),
        "allow-methods 应包含 POST，实际: {}",
        allow_methods
    );
    assert!(
        allow_methods.contains("OPTIONS"),
        "allow-methods 应包含 OPTIONS，实际: {}",
        allow_methods
    );
}

/// 测试：access-control-allow-headers 包含 content-type
///
/// OPTIONS 预检请求响应中验证 allow-headers 包含 content-type
#[tokio::test]
async fn cors_allow_headers_contains_content_type() {
    let dir = TempDir::new().unwrap();
    let state = WebState::new(dir.path().to_path_buf());
    let origins = vec!["http://localhost:5173".to_string()];
    let cors = build_cors_layer(&origins);
    let app = create_router(state).layer(cors);

    let req = Request::builder()
        .method(Method::OPTIONS)
        .uri("/health")
        .header("Origin", "http://localhost:5173")
        .header("Access-Control-Request-Method", "POST")
        .header("Access-Control-Request-Headers", "content-type")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();

    // 验证 access-control-allow-headers 包含 content-type
    let allow_headers = resp
        .headers()
        .get("access-control-allow-headers")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    assert!(
        allow_headers.to_lowercase().contains("content-type"),
        "allow-headers 应包含 content-type，实际: {}",
        allow_headers
    );
}

/// 测试：不在允许列表中的源被拒绝
///
/// 发送请求携带未授权的源，验证响应不包含 access-control-allow-origin 头
#[tokio::test]
async fn cors_rejects_unauthorized_origin() {
    let dir = TempDir::new().unwrap();
    let state = WebState::new(dir.path().to_path_buf());
    // 只允许 localhost:5173，不允许 evil.com
    let origins = vec!["http://localhost:5173".to_string()];
    let cors = build_cors_layer(&origins);
    let app = create_router(state).layer(cors);

    let req = Request::builder()
        .method(Method::GET)
        .uri("/health")
        .header("Origin", "http://evil.com")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();

    // 验证响应不包含 access-control-allow-origin 头（或头值不是 evil.com）
    let allow_origin = resp
        .headers()
        .get("access-control-allow-origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    assert!(
        allow_origin != "http://evil.com",
        "未授权源 evil.com 不应出现在 access-control-allow-origin 中"
    );
}
