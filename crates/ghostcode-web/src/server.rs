//! axum HTTP 路由器
//!
//! 组装所有 REST + SSE 路由，供 ghostcode-web 主入口和测试使用
//! Phase 6 新增：skills 端点通过 IPC client 代理到 ghostcode-daemon
//!
//! @author Atlas.oi
//! @date 2026-03-04

use std::path::PathBuf;

use axum::{
    extract::{Path, State},
    http::{header, Method as HttpMethod, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use tower_http::services::{ServeDir, ServeFile};
use ghostcode_types::ipc::DaemonRequest;
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::StreamExt;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};

use crate::handlers::{handle_active_group, handle_agents, handle_dashboard_snapshot, handle_health, handle_timeline};
use crate::ipc::{call_daemon, IpcError};
use crate::sse::tail_ledger_as_sse;
use crate::state::WebState;

/// 构建 CORS 中间件层
///
/// 根据提供的源列表创建 CorsLayer。
/// 始终包含默认源：http://localhost:5173 和 http://127.0.0.1:5173
/// 允许方法：GET, POST, OPTIONS
/// 允许头：content-type, accept
///
/// @param origins - 允许的 CORS 源列表（额外源，会与默认源合并）
/// @returns 配置好的 CorsLayer
pub fn build_cors_layer(origins: &[String]) -> CorsLayer {
    // 合并用户自定义源和默认源，避免重复
    let mut all_origins: Vec<String> = origins.to_vec();
    let defaults = ["http://localhost:5173", "http://127.0.0.1:5173"];
    for d in &defaults {
        if !all_origins.iter().any(|o| o == *d) {
            all_origins.push(d.to_string());
        }
    }

    // 将字符串源转换为 HeaderValue 列表
    // 解析失败时输出警告（而非静默丢弃），帮助排查 CORS 配置错误
    let mut origin_values: Vec<axum::http::HeaderValue> = Vec::new();
    for o in &all_origins {
        match o.parse::<axum::http::HeaderValue>() {
            Ok(v) => origin_values.push(v),
            Err(e) => {
                tracing::warn!("[GhostCode Web] CORS origin 格式非法，已跳过: {} - {}", o, e);
            }
        }
    }

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origin_values))
        .allow_methods(AllowMethods::list([
            HttpMethod::GET,
            HttpMethod::POST,
            HttpMethod::OPTIONS,
        ]))
        .allow_headers(AllowHeaders::list([header::CONTENT_TYPE, header::ACCEPT]))
}

/// 创建 axum Router（供测试和 main 共用）
///
/// 提供以下端点：
/// - GET /health - 健康检查
/// - GET /api/groups/:group_id/dashboard - Dashboard 快照
/// - GET /api/groups/:group_id/timeline - 分页时间线
/// - GET /api/groups/:group_id/agents - Agent 状态列表
/// - GET /api/groups/:group_id/stream - SSE 账本实时流
/// - GET /api/groups/:group_id/skills - 列出 Skill 候选（W1 新增）
/// - POST /api/groups/:group_id/skills/:skill_id/promote - 提升 Skill（W1 新增）
/// - GET /* - Dashboard 静态文件（SPA fallback 到 index.html）
///
/// @param state - Web 应用状态
/// @returns 配置好路由的 axum Router
pub fn create_router(state: WebState) -> Router {
    create_router_with_dashboard(state, None)
}

/// 创建带 Dashboard 静态文件服务的 axum Router
///
/// 当提供 dashboard_dir 时，将挂载 ServeDir 静态文件服务：
/// - API 路由优先匹配（/health, /api/...）
/// - 未匹配的路由回退到 Dashboard 静态文件
/// - SPA 模式：未找到的静态文件回退到 index.html（支持前端路由）
///
/// @param state - Web 应用状态
/// @param dashboard_dir - Dashboard 构建产物目录（如 ~/.ghostcode/web），None 时不挂载静态文件
/// @returns 配置好路由的 axum Router
pub fn create_router_with_dashboard(state: WebState, dashboard_dir: Option<PathBuf>) -> Router {
    let api_router = Router::new()
        // 健康检查
        .route("/health", get(handle_health))
        // 自动发现活跃 Group（前端启动时调用）
        .route("/api/active-group", get(handle_active_group))
        // Dashboard REST API
        .route(
            "/api/groups/:group_id/dashboard",
            get(handle_dashboard_snapshot),
        )
        .route("/api/groups/:group_id/timeline", get(handle_timeline))
        .route("/api/groups/:group_id/agents", get(handle_agents))
        // SSE 实时流
        .route("/api/groups/:group_id/stream", get(handle_sse_stream))
        // W1 修复：新增 skills 路由，供前端调用
        // 注意：ghostcode-web 为独立进程，暂返回空占位，后续集成 daemon IPC 调用
        .route(
            "/api/groups/:group_id/skills",
            get(handle_skills_list),
        )
        .route(
            "/api/groups/:group_id/skills/:skill_id/promote",
            post(handle_skill_promote),
        )
        .with_state(state);

    // 挂载 Dashboard 静态文件服务（SPA 模式）
    // API 路由优先匹配，未匹配的路由回退到静态文件
    // 静态文件中未找到的路径回退到 index.html（支持前端客户端路由）
    if let Some(dir) = dashboard_dir {
        let index_html = dir.join("index.html");
        let serve_dir = ServeDir::new(&dir).fallback(ServeFile::new(index_html));
        api_router.fallback_service(serve_dir)
    } else {
        api_router
    }
}

/// GET /api/groups/:group_id/stream - SSE 账本实时流
///
/// 建立 SSE 连接，持续推送账本新事件
/// 新连接从 EOF 开始 tail（只推送新事件）
///
/// @param state - 应用状态
/// @param group_id - URL 路径参数
async fn handle_sse_stream(
    State(state): State<WebState>,
    Path(group_id): Path<String>,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    // 安全校验：拒绝非法 group_id（防止路径穿越攻击）
    let ledger_path = match state.ledger_path(&group_id) {
        Some(p) => p,
        None => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({ "error": "group_id 包含非法字符" })),
            )
                .into_response();
        }
    };
    // 新连接从 EOF 开始，只推送新事件
    // C2 修复：移除命名事件类型，使用默认 message 事件
    // 前端 EventSource.onmessage 只能接收默认 message 事件，
    // 若使用 .event("ledger") 则需用 addEventListener("ledger", ...) 才能接收
    let sse_stream = tail_ledger_as_sse(ledger_path, false).map(|e| {
        Ok::<_, Infallible>(Event::default().data(e.data))
    });

    // keepalive 间隔从默认 15 秒缩短到 5 秒
    // 配合初始心跳事件，确保代理缓冲区及时冲刷
    Sse::new(sse_stream).keep_alive(
        KeepAlive::new().interval(Duration::from_secs(5))
    ).into_response()
}

/// GET /api/groups/:group_id/skills - 列出 Skill 候选
///
/// Phase 6 实现：通过 Unix Socket IPC 调用 daemon skill_list op 获取实际数据
///
/// 业务逻辑：
/// 1. 从 WebState 获取 daemon socket 路径
/// 2. 构造 DaemonRequest { op: "skill_list", args: { group_id } }
/// 3. 调用 IPC client 发送请求，等待 DaemonResponse
/// 4. daemon 返回 ok -> 200 + JSON 数据
/// 5. IPC 连接失败（daemon 不可达）-> 502 Bad Gateway
/// 6. daemon 返回错误 -> 502 Bad Gateway
///
/// @param state - 应用状态，包含 daemon_socket_path
/// @param group_id - URL 路径参数
async fn handle_skills_list(
    State(state): State<WebState>,
    Path(group_id): Path<String>,
) -> Response {
    // 构造 skill_list 请求
    let request = DaemonRequest::new(
        "skill_list",
        serde_json::json!({ "group_id": group_id }),
    );

    // 通过 IPC 调用 daemon
    match call_daemon(&state.daemon_socket_path, &request).await {
        Ok(resp) if resp.ok => {
            // daemon 返回成功，直接透传 result 字段
            Json(resp.result).into_response()
        }
        Ok(resp) => {
            // daemon 返回业务错误（ok: false）
            let error_msg = resp
                .error
                .map(|e| format!("{}: {}", e.code, e.message))
                .unwrap_or_else(|| "daemon 返回未知错误".to_string());
            tracing::warn!("skill_list daemon 返回错误: {}", error_msg);
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": error_msg })),
            )
                .into_response()
        }
        Err(IpcError::ConnectionFailed(e)) => {
            // daemon 不可达（socket 不存在或连接拒绝）
            tracing::warn!("skill_list daemon 不可达: {}", e);
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": "daemon 不可达" })),
            )
                .into_response()
        }
        Err(e) => {
            // 其他 IPC 错误
            tracing::error!("skill_list IPC 错误: {}", e);
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": format!("IPC 错误: {}", e) })),
            )
                .into_response()
        }
    }
}

/// POST /api/groups/:group_id/skills/:skill_id/promote - 提升 Skill
///
/// Phase 6 实现：通过 Unix Socket IPC 调用 daemon skill_promote op
///
/// 业务逻辑：
/// 1. 从 WebState 获取 daemon socket 路径
/// 2. 构造 DaemonRequest { op: "skill_promote", args: { group_id, skill_id } }
/// 3. 调用 IPC client 发送请求，等待 DaemonResponse
/// 4. daemon 返回 ok -> 200 + JSON 数据
/// 5. IPC 连接失败（daemon 不可达）-> 502 Bad Gateway
/// 6. daemon 返回错误 -> 502 Bad Gateway
///
/// @param state - 应用状态，包含 daemon_socket_path
/// @param path - URL 路径参数（group_id, skill_id）
async fn handle_skill_promote(
    State(state): State<WebState>,
    Path((group_id, skill_id)): Path<(String, String)>,
) -> Response {
    // 构造 skill_promote 请求
    let request = DaemonRequest::new(
        "skill_promote",
        serde_json::json!({ "group_id": group_id, "skill_id": skill_id }),
    );

    // 通过 IPC 调用 daemon
    match call_daemon(&state.daemon_socket_path, &request).await {
        Ok(resp) if resp.ok => {
            // daemon 返回成功，直接透传 result 字段
            Json(resp.result).into_response()
        }
        Ok(resp) => {
            // daemon 返回业务错误（ok: false）
            let error_msg = resp
                .error
                .map(|e| format!("{}: {}", e.code, e.message))
                .unwrap_or_else(|| "daemon 返回未知错误".to_string());
            tracing::warn!("skill_promote daemon 返回错误: {}", error_msg);
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": error_msg })),
            )
                .into_response()
        }
        Err(IpcError::ConnectionFailed(e)) => {
            // daemon 不可达（socket 不存在或连接拒绝）
            tracing::warn!("skill_promote daemon 不可达: {}", e);
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": "daemon 不可达" })),
            )
                .into_response()
        }
        Err(e) => {
            // 其他 IPC 错误
            tracing::error!("skill_promote IPC 错误: {}", e);
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": format!("IPC 错误: {}", e) })),
            )
                .into_response()
        }
    }
}
