//! Dashboard REST Handler 实现
//!
//! 处理 /api/groups/:id/dashboard、/timeline、/agents 三个端点
//! 通过 ghostcode-ledger::query 查询账本数据
//!
//! @author Atlas.oi
//! @date 2026-03-03

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use ghostcode_ledger::query::{aggregate_agent_status, build_history_projection, timeline_page};

use crate::state::WebState;

/// 分页查询参数
#[derive(Deserialize)]
pub struct TimelineQuery {
    /// 每页条目数（默认 20，最大 100）
    pub page_size: Option<usize>,
    /// 分页游标（从上一页末尾 event id）
    pub cursor: Option<String>,
}

/// GET /health - 健康检查
///
/// 返回服务器版本信息，用于探活和版本确认
pub async fn handle_health() -> impl IntoResponse {
    Json(serde_json::json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") }))
}

/// GET /api/groups/:group_id/dashboard - 获取 Dashboard 快照
///
/// 业务逻辑：
/// 1. 从 URL 路径提取 group_id
/// 2. 构造账本路径
/// 3. 调用 build_history_projection 生成快照
/// 4. 账本不存在（NotFound）时返回空快照；其他 IO 错误返回 500
///
/// C3 修复：直接返回裸 DTO，不包装在 ok/data 中
/// W4 修复：区分文件不存在（返回空数据）和其他 IO 错误（返回 500）
///
/// @param state - 应用状态（含数据根目录）
/// @param group_id - URL 路径参数
pub async fn handle_dashboard_snapshot(
    State(state): State<WebState>,
    Path(group_id): Path<String>,
) -> impl IntoResponse {
    let ledger_path = state.ledger_path(&group_id);
    // 账本不存在时直接返回空快照（正常情况，不报错）
    if !ledger_path.exists() {
        let empty = ghostcode_types::dashboard::DashboardSnapshot {
            group_id,
            snapshot_ts: chrono::Utc::now()
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            total_events: 0,
            agents: vec![],
            recent_timeline: vec![],
        };
        return Json(empty).into_response();
    }
    match build_history_projection(&ledger_path, &group_id) {
        // C3 修复：直接返回裸 snapshot，不包装
        Ok(snapshot) => Json(snapshot).into_response(),
        // W4 修复：其他 IO 错误返回 500
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// GET /api/groups/:group_id/timeline - 分页时间线
///
/// 业务逻辑：
/// 1. 从 URL 路径提取 group_id，从查询参数提取分页参数
/// 2. 调用 timeline_page 查询
/// 3. 账本不存在时返回空分页；其他 IO 错误返回 500
///
/// C3 修复：直接返回裸 DTO
/// W4 修复：区分文件不存在和其他错误
///
/// @param state - 应用状态
/// @param group_id - URL 路径参数
/// @param params - 查询参数（page_size, cursor）
pub async fn handle_timeline(
    State(state): State<WebState>,
    Path(group_id): Path<String>,
    Query(params): Query<TimelineQuery>,
) -> impl IntoResponse {
    let ledger_path = state.ledger_path(&group_id);
    let page_size = params.page_size.unwrap_or(20).min(100);
    // 账本不存在时直接返回空分页（正常情况，不报错）
    if !ledger_path.exists() {
        let empty = ghostcode_types::dashboard::TimelinePage {
            items: vec![],
            next_cursor: None,
            total: 0,
        };
        return Json(empty).into_response();
    }
    match timeline_page(&ledger_path, page_size, params.cursor) {
        // C3 修复：直接返回裸 page
        Ok(page) => Json(page).into_response(),
        // W4 修复：其他 IO 错误返回 500
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// GET /api/groups/:group_id/agents - 获取 Agent 状态列表
///
/// 业务逻辑：
/// 1. 从 URL 路径提取 group_id
/// 2. 调用 aggregate_agent_status 统计
/// 3. 账本不存在时返回空列表；其他 IO 错误返回 500
///
/// C3 修复：直接返回裸 DTO
/// W4 修复：区分文件不存在和其他错误
///
/// @param state - 应用状态
/// @param group_id - URL 路径参数
pub async fn handle_agents(
    State(state): State<WebState>,
    Path(group_id): Path<String>,
) -> impl IntoResponse {
    let ledger_path = state.ledger_path(&group_id);
    // 账本不存在时直接返回空列表（正常情况，不报错）
    if !ledger_path.exists() {
        return Json(Vec::<ghostcode_types::dashboard::AgentStatusView>::new()).into_response();
    }
    match aggregate_agent_status(&ledger_path) {
        // C3 修复：直接返回裸 agents 列表
        Ok(agents) => Json(agents).into_response(),
        // W4 修复：其他 IO 错误返回 500
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
