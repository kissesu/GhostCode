//! Dashboard Handler 实现
//!
//! 提供 Dashboard 三个 op 的 handler 函数：
//! - handle_dashboard_snapshot: 构建 Group 完整 Dashboard 快照
//! - handle_dashboard_timeline: 分页读取账本时间线
//! - handle_dashboard_agents: 统计 Agent 状态
//!
//! 账本路径由 AppState.groups_dir + group_id 构造：
//! {groups_dir}/{group_id}/state/ledger/ledger.jsonl
//!
//! 账本文件不存在时优雅返回空结果（不报错）
//!
//! @author Atlas.oi
//! @date 2026-03-03

use std::path::PathBuf;

use ghostcode_ledger::query;
use ghostcode_types::dashboard::{DashboardSnapshot, TimelinePage};
use ghostcode_types::ipc::DaemonResponse;

use crate::server::AppState;

/// 构造账本文件路径
///
/// 格式: {groups_dir}/{group_id}/state/ledger/ledger.jsonl
///
/// @param state - 共享应用状态
/// @param group_id - Group ID
/// @returns 账本文件绝对路径
fn ledger_path(state: &AppState, group_id: &str) -> PathBuf {
    state.groups_dir.join(group_id).join("state/ledger/ledger.jsonl")
}

/// 校验 group_id 是否合法，防止路径穿越攻击
///
/// 安全规则：
/// 1. 不允许为空
/// 2. 不允许包含 `..`、`/`、`\` 等路径穿越字符
/// 3. 仅允许字母、数字、连字符、下划线
///
/// @param group_id - 待校验的 Group ID 字符串
/// @returns Ok(()) 表示合法，Err(DaemonResponse) 包含具体错误
fn validate_group_id(group_id: &str) -> Result<(), DaemonResponse> {
    if group_id.is_empty() {
        return Err(DaemonResponse::err("INVALID_ARGS", "group_id 不能为空"));
    }
    // 禁止路径穿越特殊字符
    if group_id.contains("..") || group_id.contains('/') || group_id.contains('\\') {
        return Err(DaemonResponse::err(
            "INVALID_ARGS",
            format!("group_id 包含非法字符（不允许 ..、/、\\）: \"{}\"", group_id),
        ));
    }
    // 仅允许字母数字、连字符、下划线
    if !group_id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(DaemonResponse::err(
            "INVALID_ARGS",
            format!(
                "group_id 只允许字母、数字、连字符、下划线: \"{}\"",
                group_id
            ),
        ));
    }
    Ok(())
}

/// dashboard_snapshot handler
///
/// 构建 Group 的完整 Dashboard 快照，包含 Agent 状态和最近时间线
///
/// 业务逻辑：
/// 1. 提取必填参数 group_id
/// 2. 构造账本路径
/// 3. 调用 build_history_projection 生成快照
/// 4. 账本不存在时返回空快照（优雅降级）
///
/// 必填参数：group_id
///
/// @param state - 共享应用状态
/// @param args - 请求参数
pub async fn handle_dashboard_snapshot(
    state: &AppState,
    args: &serde_json::Value,
) -> DaemonResponse {
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };

    // C1 安全修复：校验 group_id 防止路径穿越攻击
    if let Err(resp) = validate_group_id(group_id) {
        return resp;
    }

    let path = ledger_path(state, group_id);

    // 账本文件不存在时返回空快照
    if !path.exists() {
        let empty = DashboardSnapshot {
            group_id: group_id.to_string(),
            snapshot_ts: chrono::Utc::now()
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            total_events: 0,
            agents: vec![],
            recent_timeline: vec![],
        };
        return DaemonResponse::ok(serde_json::to_value(empty).unwrap_or_default());
    }

    match query::build_history_projection(&path, group_id) {
        Ok(snapshot) => DaemonResponse::ok(serde_json::to_value(snapshot).unwrap_or_default()),
        Err(e) => DaemonResponse::err("LEDGER_ERROR", e.to_string()),
    }
}

/// dashboard_timeline handler
///
/// 分页读取账本时间线事件
///
/// 业务逻辑：
/// 1. 提取必填参数 group_id 和可选参数 page_size / cursor
/// 2. 构造账本路径
/// 3. 调用 timeline_page 分页查询
/// 4. 账本不存在时返回空分页结果
///
/// 必填参数：group_id
/// 可选参数：page_size（默认 20，最大 100）, cursor
///
/// @param state - 共享应用状态
/// @param args - 请求参数
pub async fn handle_dashboard_timeline(
    state: &AppState,
    args: &serde_json::Value,
) -> DaemonResponse {
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };

    // C1 安全修复：校验 group_id 防止路径穿越攻击
    if let Err(resp) = validate_group_id(group_id) {
        return resp;
    }

    let page_size = args["page_size"].as_u64().unwrap_or(20) as usize;
    let cursor = args["cursor"].as_str().map(String::from);

    let path = ledger_path(state, group_id);

    // 账本文件不存在时返回空分页
    if !path.exists() {
        let empty = TimelinePage {
            items: vec![],
            next_cursor: None,
            total: 0,
        };
        return DaemonResponse::ok(serde_json::to_value(empty).unwrap_or_default());
    }

    match query::timeline_page(&path, page_size, cursor) {
        Ok(page) => DaemonResponse::ok(serde_json::to_value(page).unwrap_or_default()),
        Err(e) => DaemonResponse::err("LEDGER_ERROR", e.to_string()),
    }
}

/// dashboard_agents handler
///
/// 统计 Group 中每个 Agent 的最后已知状态
///
/// 业务逻辑：
/// 1. 提取必填参数 group_id
/// 2. 构造账本路径
/// 3. 调用 aggregate_agent_status 统计
/// 4. 账本不存在时返回空列表
///
/// 必填参数：group_id
///
/// @param state - 共享应用状态
/// @param args - 请求参数
pub async fn handle_dashboard_agents(
    state: &AppState,
    args: &serde_json::Value,
) -> DaemonResponse {
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };

    // C1 安全修复：校验 group_id 防止路径穿越攻击
    if let Err(resp) = validate_group_id(group_id) {
        return resp;
    }

    let path = ledger_path(state, group_id);

    // 账本文件不存在时返回空列表
    if !path.exists() {
        return DaemonResponse::ok(serde_json::json!({ "agents": [] }));
    }

    match query::aggregate_agent_status(&path) {
        Ok(agents) => DaemonResponse::ok(serde_json::json!({ "agents": agents })),
        Err(e) => DaemonResponse::err("LEDGER_ERROR", e.to_string()),
    }
}
