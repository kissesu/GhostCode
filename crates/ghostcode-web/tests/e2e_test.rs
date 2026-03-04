//! ghostcode-web Phase 5/6 端到端集成测试
//!
//! 验证 Web HTTP API 端到端链路：
//! - 场景 1: HTTP API 集成（含有事件数据的账本）
//! - 场景 2: SSE 流集成（验证 SSE 端点可建立连接）
//! - 场景 3: IPC client + Skills 端点（使用假 Unix Socket daemon helper）
//!
//! Web API 响应格式: 裸 DTO（C3 修复后不再包装 ok/data）
//! 账本路径规则（与 WebState.ledger_path 一致）：
//! {data_root}/groups/{group_id}/ledger.ndjson
//!
//! @author Atlas.oi
//! @date 2026-03-04

use axum::body::Body;
use axum::http::{Request, StatusCode};
use ghostcode_ledger::append_event;
use ghostcode_types::event::{Event, EventKind};
use ghostcode_web::server::create_router;
use ghostcode_web::state::WebState;
use serde_json::json;
use tempfile::TempDir;
use tower::ServiceExt;

// ============================================
// 测试辅助函数
// ============================================

/// 创建 WebState 并写入测试事件，返回 (TempDir, WebState)
///
/// 账本路径: {data_root}/groups/{group_id}/ledger.ndjson
/// TempDir 必须持有直到测试结束，防止目录被提前清理
fn setup_web_with_events(group_id: &str, event_count: usize) -> (TempDir, WebState) {
    let dir = TempDir::new().expect("创建临时目录失败");
    let state = WebState::new(dir.path().to_path_buf());

    // WebState.ledger_path 返回: {data_root}/groups/{group_id}/ledger.ndjson
    let ledger = state.ledger_path(group_id);
    let lock = ledger.with_extension("lock");

    // 创建 groups/{group_id}/ 目录层级
    std::fs::create_dir_all(ledger.parent().expect("账本父目录应存在")).expect("创建目录失败");

    // 写入测试事件，使用多种 EventKind
    let kinds = [
        EventKind::ChatMessage,
        EventKind::ActorStart,
        EventKind::SystemNotify,
        EventKind::SkillLearned,
        EventKind::SkillPromoted,
    ];

    for i in 0..event_count {
        let kind = kinds[i % kinds.len()].clone();
        let actor = format!("actor-{}", i % 2);
        let event = Event::new(
            kind,
            group_id,
            &format!("session-{}", i),
            &actor,
            json!({ "index": i, "text": format!("事件 {}", i) }),
        );
        append_event(&ledger, &lock, &event).expect("追加事件失败");
    }

    (dir, state)
}

/// 解析响应体 JSON，直接返回裸 DTO（C3 修复后格式）
///
/// C3 修复后，REST handler 直接返回裸 DTO，不再包装在 {"ok": true, "data": ...} 中
async fn parse_api_data(resp: axum::http::Response<axum::body::Body>) -> serde_json::Value {
    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("读取响应体失败");
    let body: serde_json::Value =
        serde_json::from_slice(&body_bytes).expect("响应体应为有效 JSON");
    // C3 修复：直接返回裸 DTO，不再期望 ok/data 包装结构
    body
}

// ============================================
// 场景 1：HTTP API 集成测试（含数据）
// ============================================

/// 验证 GET /api/groups/:id/dashboard 在有数据时返回正确的 JSON
#[tokio::test]
async fn e2e_dashboard_endpoint_with_data() {
    let group_id = "e2e-group-001";
    let (_dir, state) = setup_web_with_events(group_id, 5);
    let app = create_router(state);

    let req = Request::builder()
        .uri(format!("/api/groups/{}/dashboard", group_id))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK, "dashboard 端点应返回 200");

    // C3 修复：直接解析裸 DashboardSnapshot（不再包装 ok/data）
    let data = parse_api_data(resp).await;

    // 验证 DashboardSnapshot 结构包含 group_id 和 total_events
    assert!(
        data.get("group_id").is_some(),
        "dashboard data 应包含 group_id，实际: {}",
        data
    );
    assert!(
        data.get("total_events").is_some(),
        "dashboard data 应包含 total_events，实际: {}",
        data
    );

    let total_events = data["total_events"].as_u64().unwrap_or(0);
    assert_eq!(total_events, 5, "写入 5 个事件，total_events 应为 5");
}

/// 验证 GET /api/groups/:id/timeline 在有数据时返回分页事件列表
#[tokio::test]
async fn e2e_timeline_endpoint_with_data() {
    let group_id = "e2e-group-002";
    let (_dir, state) = setup_web_with_events(group_id, 6);
    let app = create_router(state);

    let req = Request::builder()
        .uri(format!("/api/groups/{}/timeline", group_id))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK, "timeline 端点应返回 200");

    // C3 修复：直接解析裸 TimelinePage（不再包装 ok/data）
    let data = parse_api_data(resp).await;

    // 验证 TimelinePage 结构（含 items 和 total）
    assert!(
        data.get("items").is_some(),
        "timeline data 应包含 items 字段，实际: {}",
        data
    );

    let total = data["total"].as_u64().unwrap_or(0);
    assert_eq!(total, 6, "写入 6 个事件，total 应为 6");

    // items 数组应非空
    let items_len = data["items"].as_array().map(|a| a.len()).unwrap_or(0);
    assert!(items_len > 0, "items 数组不应为空，实际长度: {}", items_len);
}

/// 验证 GET /api/groups/:id/agents 在有数据时返回 agent 状态
#[tokio::test]
async fn e2e_agents_endpoint_with_data() {
    let group_id = "e2e-group-003";
    let (_dir, state) = setup_web_with_events(group_id, 4);
    let app = create_router(state);

    let req = Request::builder()
        .uri(format!("/api/groups/{}/agents", group_id))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK, "agents 端点应返回 200");

    // C3 修复：agents 端点直接返回裸 [AgentStatusView] 数组
    let data = parse_api_data(resp).await;

    // data 应为数组（不是含 agents 字段的对象）
    assert!(
        data.is_array(),
        "agents data 应为数组，实际: {}",
        data
    );

    let agents = data.as_array().unwrap();
    assert!(!agents.is_empty(), "有事件的 group agents 列表不应为空");
}

/// 验证多个 group 的账本数据相互隔离
#[tokio::test]
async fn e2e_groups_data_isolation() {
    let group_a = "e2e-isolation-a";
    let group_b = "e2e-isolation-b";

    let dir = TempDir::new().expect("创建临时目录失败");
    let state = WebState::new(dir.path().to_path_buf());

    // 写入 group_a：3 个事件
    let ledger_a = state.ledger_path(group_a);
    std::fs::create_dir_all(ledger_a.parent().unwrap()).unwrap();
    let lock_a = ledger_a.with_extension("lock");
    for i in 0..3 {
        let ev = Event::new(EventKind::ChatMessage, group_a, "s1", "actor-a", json!({"i": i}));
        append_event(&ledger_a, &lock_a, &ev).unwrap();
    }

    // 写入 group_b：7 个事件
    let ledger_b = state.ledger_path(group_b);
    std::fs::create_dir_all(ledger_b.parent().unwrap()).unwrap();
    let lock_b = ledger_b.with_extension("lock");
    for i in 0..7 {
        let ev = Event::new(EventKind::ChatMessage, group_b, "s1", "actor-b", json!({"i": i}));
        append_event(&ledger_b, &lock_b, &ev).unwrap();
    }

    // 查询 group_a timeline 并验证 total
    let app_a = create_router(state.clone());
    let req_a = Request::builder()
        .uri(format!("/api/groups/{}/timeline", group_a))
        .body(Body::empty())
        .unwrap();
    let resp_a = app_a.oneshot(req_a).await.unwrap();
    assert_eq!(resp_a.status(), StatusCode::OK);
    let data_a = parse_api_data(resp_a).await;
    let total_a = data_a["total"].as_u64().unwrap_or(0);

    // 查询 group_b timeline 并验证 total
    let app_b = create_router(state.clone());
    let req_b = Request::builder()
        .uri(format!("/api/groups/{}/timeline", group_b))
        .body(Body::empty())
        .unwrap();
    let resp_b = app_b.oneshot(req_b).await.unwrap();
    assert_eq!(resp_b.status(), StatusCode::OK);
    let data_b = parse_api_data(resp_b).await;
    let total_b = data_b["total"].as_u64().unwrap_or(0);

    assert_eq!(total_a, 3, "group_a 应有 3 个事件，实际: {}", total_a);
    assert_eq!(total_b, 7, "group_b 应有 7 个事件，实际: {}", total_b);
}

// ============================================
// 场景 2：SSE 流集成
// ============================================

/// 验证 SSE 端点在有账本数据时返回 text/event-stream 响应
#[tokio::test]
async fn e2e_sse_endpoint_connects_with_data() {
    let group_id = "e2e-sse-group";
    let (_dir, state) = setup_web_with_events(group_id, 2);
    let app = create_router(state);

    let req = Request::builder()
        .uri(format!("/api/groups/{}/stream", group_id))
        .header("Accept", "text/event-stream")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    // SSE 端点应返回 200 并设置正确的 Content-Type
    assert_eq!(resp.status(), StatusCode::OK, "SSE 端点应返回 200");

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.contains("text/event-stream"),
        "SSE 端点应返回 text/event-stream，实际: {}",
        content_type
    );
}

/// 验证 SSE 端点在账本不存在时也能正常响应（不崩溃）
#[tokio::test]
async fn e2e_sse_nonexistent_group_no_crash() {
    let dir = TempDir::new().expect("创建临时目录失败");
    let state = WebState::new(dir.path().to_path_buf());
    let app = create_router(state);

    let req = Request::builder()
        .uri("/api/groups/nonexistent-group/stream")
        .header("Accept", "text/event-stream")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    // 账本不存在时 SSE 端点也应返回 200（等待新事件）
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "账本不存在时 SSE 端点不应崩溃，应返回 200"
    );
}

// ============================================
// 场景 3：IPC client + Skills 端点全链路测试
// ============================================

use ghostcode_types::ipc::DaemonResponse;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

/// 启动一个假的 daemon Unix Socket 服务器，接收一个请求并返回固定响应
///
/// 业务逻辑：
/// 1. 在指定 socket 路径上监听
/// 2. 接受一个连接
/// 3. 读取一行 JSON 请求（DaemonRequest）
/// 4. 根据 op 返回预定义的 DaemonResponse
/// 5. 关闭连接
async fn fake_daemon_server(socket_path: std::path::PathBuf, response: DaemonResponse) {
    let listener = UnixListener::bind(&socket_path).expect("假 daemon 绑定 socket 失败");
    // 接受一个连接，处理一次请求后退出
    if let Ok((stream, _)) = listener.accept().await {
        let (reader_half, mut writer_half) = stream.into_split();
        let mut reader = BufReader::new(reader_half);
        let mut line = String::new();
        // 读取一行请求
        let _ = reader.read_line(&mut line).await;
        // 返回预定义响应（以换行符结束）
        let resp_json = serde_json::to_string(&response).unwrap() + "\n";
        let _ = writer_half.write_all(resp_json.as_bytes()).await;
    }
}

/// 验证 skills_list 端点通过 IPC 代理 daemon 的 skill_list 操作
///
/// 业务逻辑：
/// 1. 启动假 daemon 服务器，返回预设 skill 列表
/// 2. WebState 配置假 daemon 的 socket 路径
/// 3. 发送 GET /api/groups/:id/skills 请求
/// 4. 验证响应包含 daemon 返回的 skill 数据
#[tokio::test]
async fn skills_list_proxies_daemon_skill_list() {
    let dir = TempDir::new().expect("创建临时目录失败");
    // 使用 TempDir 内的 socket 路径，避免文件名冲突
    let socket_path = dir.path().join("daemon-skills-list.sock");

    // 预定义假 daemon 的响应数据
    let skills_data = json!([
        {"id": "skill-001", "name": "rust-expert", "score": 0.9},
        {"id": "skill-002", "name": "async-patterns", "score": 0.8}
    ]);
    let fake_resp = DaemonResponse::ok(skills_data.clone());

    // 先绑定 socket（避免 race condition：server 还没监听，client 就连接了）
    let socket_path_clone = socket_path.clone();
    let server_handle = tokio::spawn(async move {
        fake_daemon_server(socket_path_clone, fake_resp).await;
    });

    // 短暂等待 server 就绪
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // 创建 WebState，指向假 daemon socket
    let state = WebState::with_socket(dir.path().to_path_buf(), socket_path);
    let app = create_router(state);

    let req = Request::builder()
        .uri("/api/groups/test-group/skills")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    // 应返回 200，且包含 daemon 返回的 skill 数据
    assert_eq!(resp.status(), StatusCode::OK, "skills_list 应返回 200");

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("读取响应体失败");
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).expect("响应应为 JSON");

    // 验证数据包含 skill 列表（daemon 返回的 result 字段）
    assert!(body.is_array() || body.get("skills").is_some() || body == skills_data,
        "skills_list 响应应包含 skill 数据，实际: {}", body);

    // 等待假 daemon server 退出
    let _ = server_handle.await;
}

/// 验证 skill_promote 端点通过 IPC 代理 daemon 的 skill_promote 操作
///
/// 业务逻辑：
/// 1. 启动假 daemon 服务器，返回 promote 成功响应
/// 2. WebState 配置假 daemon 的 socket 路径
/// 3. 发送 POST /api/groups/:id/skills/:skill_id/promote 请求
/// 4. 验证响应包含 daemon 返回的结果
#[tokio::test]
async fn skill_promote_proxies_daemon_and_returns_result() {
    let dir = TempDir::new().expect("创建临时目录失败");
    let socket_path = dir.path().join("daemon-promote.sock");

    // 预定义假 daemon 的 promote 响应
    let promote_result = json!({"accepted": true, "skill_id": "skill-001"});
    let fake_resp = DaemonResponse::ok(promote_result.clone());

    let socket_path_clone = socket_path.clone();
    let server_handle = tokio::spawn(async move {
        fake_daemon_server(socket_path_clone, fake_resp).await;
    });

    // 短暂等待 server 就绪
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let state = WebState::with_socket(dir.path().to_path_buf(), socket_path);
    let app = create_router(state);

    let req = Request::builder()
        .uri("/api/groups/test-group/skills/skill-001/promote")
        .method("POST")
        .header("Content-Type", "application/json")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    // 应返回 200，且包含 daemon 返回的 promote 结果
    assert_eq!(resp.status(), StatusCode::OK, "skill_promote 应返回 200");

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("读取响应体失败");
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).expect("响应应为 JSON");

    // 验证响应包含 accepted 字段
    assert!(
        body.get("accepted").and_then(|v| v.as_bool()).unwrap_or(false),
        "skill_promote 响应应包含 accepted: true，实际: {}",
        body
    );

    let _ = server_handle.await;
}
