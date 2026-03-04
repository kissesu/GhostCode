//! axum HTTP 路由器
//!
//! 组装所有 REST + SSE 路由，供 ghostcode-web 主入口和测试使用
//! Phase 6 新增：skills 端点通过 IPC client 代理到 ghostcode-daemon
//!
//! @author Atlas.oi
//! @date 2026-03-04

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use ghostcode_types::ipc::DaemonRequest;
use std::convert::Infallible;
use tokio_stream::StreamExt;

use crate::handlers::{handle_agents, handle_dashboard_snapshot, handle_health, handle_timeline};
use crate::ipc::{call_daemon, IpcError};
use crate::sse::tail_ledger_as_sse;
use crate::state::WebState;

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
///
/// @param state - Web 应用状态
/// @returns 配置好路由的 axum Router
pub fn create_router(state: WebState) -> Router {
    Router::new()
        // 健康检查
        .route("/health", get(handle_health))
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
        .with_state(state)
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
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let ledger_path = state.ledger_path(&group_id);
    // 新连接从 EOF 开始，只推送新事件
    // C2 修复：移除命名事件类型，使用默认 message 事件
    // 前端 EventSource.onmessage 只能接收默认 message 事件，
    // 若使用 .event("ledger") 则需用 addEventListener("ledger", ...) 才能接收
    let sse_stream = tail_ledger_as_sse(ledger_path, false).map(|e| {
        Ok::<_, Infallible>(Event::default().data(e.data))
    });

    Sse::new(sse_stream).keep_alive(KeepAlive::default())
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
