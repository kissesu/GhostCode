//! axum HTTP 路由器
//!
//! 组装所有 REST + SSE 路由，供 ghostcode-web 主入口和测试使用
//!
//! @author Atlas.oi
//! @date 2026-03-03

use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use std::convert::Infallible;
use tokio_stream::StreamExt;

use crate::handlers::{handle_agents, handle_dashboard_snapshot, handle_health, handle_timeline};
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
/// W1 修复：为前端提供 skills 端点
/// 注意：ghostcode-web 是独立进程，不直接访问 daemon 内存状态
/// 当前返回空数组占位，后续通过 IPC 调用 daemon 的 skill_list op 获取真实数据
///
/// @param _state - 应用状态（当前未使用，占位备用）
/// @param _group_id - URL 路径参数（当前未使用）
async fn handle_skills_list(
    State(_state): State<WebState>,
    Path(_group_id): Path<String>,
) -> Json<serde_json::Value> {
    // TODO：后续通过 Unix Socket IPC 调用 daemon skill_list op 获取实际数据
    Json(serde_json::json!([]))
}

/// POST /api/groups/:group_id/skills/:skill_id/promote - 提升 Skill
///
/// W1 修复：为前端提供 skill promote 端点
/// 注意：ghostcode-web 是独立进程，不直接访问 daemon 内存状态
/// 当前返回 accepted 占位，后续通过 IPC 调用 daemon 的 skill_promote op
///
/// @param _state - 应用状态（当前未使用，占位备用）
/// @param path - URL 路径参数（group_id, skill_id）
async fn handle_skill_promote(
    State(_state): State<WebState>,
    Path((_group_id, skill_id)): Path<(String, String)>,
) -> Json<serde_json::Value> {
    // TODO：后续通过 Unix Socket IPC 调用 daemon skill_promote op 执行真实提升
    Json(serde_json::json!({ "accepted": true, "skill_id": skill_id }))
}
