//! 请求分发框架
//!
//! 将 DaemonRequest 的 op 字符串路由到对应的 handler 函数
//! Phase 1 支持 21 个 op，其中 ping/shutdown/actor_start/actor_stop/
//! headless_status/headless_set_status 已实现，其余为占位
//!
//! 参考: cccc/src/cccc/daemon/request_dispatch_ops.py - 责任链模式
//!
//! @author Atlas.oi
//! @date 2026-03-01

use std::sync::Arc;
use std::time::Duration;

use ghostcode_router::backend::{ClaudeBackend, CodexBackend, GeminiBackend, TaskConfig, TaskMode, Backend};
use ghostcode_router::dag::{topological_sort, TaskNode};
use ghostcode_router::executor::{Executor, ExecutableTask, ExecutorConfig, TaskStatus};
use ghostcode_router::process::should_use_stdin;
use ghostcode_router::session::SessionKey;
use ghostcode_router::stream::StreamParser;
use ghostcode_router::task_format::parse_task_format;
use ghostcode_types::ipc::{DaemonRequest, DaemonResponse};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::routing::RoutingState;
use crate::server::AppState;
use crate::verification::VerifyEvent;
use crate::{lifecycle, messaging::{send, inbox}, runner::HeadlessStatus};

/// Phase 1 + Phase 2 + Phase 3 + Phase 4 所有已知的 op 字符串（40 个）
pub const KNOWN_OPS: &[&str] = &[
    // 核心
    "ping",
    "shutdown",
    // Group 管理
    "group_create",
    "group_show",
    "group_start",
    "group_stop",
    "group_delete",
    "group_set_state",
    "groups",
    // Actor 管理
    "actor_add",
    "actor_list",
    "actor_start",
    "actor_stop",
    "actor_remove",
    // 消息
    "send",
    "reply",
    "inbox_list",
    "inbox_mark_read",
    "inbox_mark_all_read",
    // Headless
    "headless_status",
    "headless_set_status",
    // 路由（Phase 2）
    "route_task",
    "route_task_parallel",
    "route_status",
    "route_cancel",
    "session_list",
    // 验证（Phase 3）
    "verification_start",
    "verification_status",
    "verification_cancel",
    // HUD（Phase 3）
    "hud_snapshot",
    // Dashboard（Phase 4）
    "dashboard_snapshot",
    "dashboard_timeline",
    "dashboard_agents",
    // Skill Learning（Phase 4）
    "skill_extract",
    "skill_list",
    "skill_promote",
    "skill_learn_fragment",
    "team_skill_list",
];

/// 分发请求到对应处理器
///
/// 根据 req.op 匹配到对应的 handler 函数
/// 未知 op 返回明确错误（不 panic）
///
/// @param state - 共享应用状态
/// @param req - Daemon 请求
/// @return DaemonResponse
pub async fn dispatch(state: &AppState, req: DaemonRequest) -> DaemonResponse {
    match req.op.as_str() {
        // === 核心 ===
        "ping" => handle_ping(state, &req.args).await,
        "shutdown" => handle_shutdown(state),

        // === Group 管理 ===
        "group_create" => handle_group_create(state, &req.args),
        "group_show" => handle_group_show(state, &req.args),
        // group_start/stop 通过 set_group_state 实现（仅状态变更，无 OS 进程管理）
        // group_start 将 Group 状态置为 active，group_stop 置为 idle
        "group_start" => handle_group_set_state_shortcut(state, &req.args, "active"),
        "group_stop" => handle_group_set_state_shortcut(state, &req.args, "idle"),
        "group_delete" => handle_group_delete(state, &req.args),
        "group_set_state" => handle_group_set_state(state, &req.args),
        "groups" => handle_groups_list(state),

        // === Actor 管理 ===
        "actor_add" => handle_actor_add(state, &req.args),
        "actor_list" => handle_actor_list(state, &req.args),
        "actor_start" => handle_actor_start(state, &req.args).await,
        "actor_stop" => handle_actor_stop(state, &req.args).await,
        "actor_remove" => handle_actor_remove(state, &req.args),

        // === 消息 ===
        "send" => handle_send(state, &req.args).await,
        "reply" => handle_reply(state, &req.args).await,
        "inbox_list" => handle_inbox_list(state, &req.args).await,
        "inbox_mark_read" => handle_inbox_mark_read(state, &req.args).await,
        "inbox_mark_all_read" => handle_inbox_mark_all_read(state, &req.args).await,

        // === Headless ===
        "headless_status" => handle_headless_status(state, &req.args).await,
        "headless_set_status" => handle_headless_set_status(state, &req.args).await,

        // === 路由（Phase 2） ===
        "route_task" => handle_route_task(state, &req.args).await,
        "route_task_parallel" => handle_route_task_parallel(state, &req.args).await,
        "route_status" => handle_route_status(state, &req.args).await,
        "route_cancel" => handle_route_cancel(state, &req.args).await,
        "session_list" => handle_session_list(state, &req.args).await,

        // === 验证（Phase 3） ===
        "verification_start" => handle_verification_start(state, &req.args),
        "verification_status" => handle_verification_status(state, &req.args),
        "verification_cancel" => handle_verification_cancel(state, &req.args),

        // === HUD（Phase 3） ===
        "hud_snapshot" => handle_hud_snapshot(state, &req.args),

        // === Dashboard（Phase 4）===
        "dashboard_snapshot" => crate::dashboard::handle_dashboard_snapshot(state, &req.args).await,
        "dashboard_timeline" => crate::dashboard::handle_dashboard_timeline(state, &req.args).await,
        "dashboard_agents" => crate::dashboard::handle_dashboard_agents(state, &req.args).await,
        // === Skill Learning（Phase 4）===
        // Phase 6 Task 5：skill_extract 已升级为启发式实现，不再依赖 LLM 调用链
        "skill_extract" => handle_skill_extract(state, &req.args),
        "skill_list" => handle_skill_list(state, &req.args),
        "skill_promote" => handle_skill_promote(state, &req.args),
        "skill_learn_fragment" => handle_skill_learn_fragment(state, &req.args),
        // P9-T2：team_skill_list 正式实现，聚合所有 group 的候选技能
        "team_skill_list" => handle_team_skill_list(state, &req.args),

        // === 未知 op ===
        _ => DaemonResponse::err("UNKNOWN_OP", format!("unknown operation: {}", req.op)),
    }
}

// ============================================
// Handler 实现
// ============================================

/// ping handler [ERR-3]
///
/// 返回版本信息和未读消息状态
/// has_unread 字段由 DeliveryEngine 维护，反映当前 Actor 是否有待读消息
///
/// 参数：
/// - group_id（可选）：Actor 所在 Group ID
/// - actor_id（可选）：查询 has_unread 的 Actor ID
async fn handle_ping(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    // 提取可选参数（ping 不要求必填，无则返回 has_unread=false）
    let has_unread = if let (Some(group_id), Some(actor_id)) = (
        args["group_id"].as_str(),
        args["actor_id"].as_str(),
    ) {
        state.delivery.throttle.has_unread(group_id, actor_id)
    } else {
        false
    };

    DaemonResponse::ok(serde_json::json!({
        "pong": true,
        "version": env!("CARGO_PKG_VERSION"),
        "has_unread": has_unread
    }))
}

/// shutdown handler
///
/// 触发 Daemon 优雅关闭
fn handle_shutdown(state: &AppState) -> DaemonResponse {
    state.trigger_shutdown();
    DaemonResponse::ok(serde_json::json!({ "shutting_down": true }))
}

/// actor_start handler
///
/// 启动指定 Actor，创建 HeadlessSession
///
/// 必填参数：group_id, actor_id
///
/// @param state - 共享应用状态
/// @param args - 请求参数
async fn handle_actor_start(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    // 提取必填参数
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };
    let actor_id = match args["actor_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: actor_id"),
    };
    // 输入格式验证（防止路径遍历）
    if let Err(resp) = validate_id(group_id, "group_id") { return resp; }
    if let Err(resp) = validate_id(actor_id, "actor_id") { return resp; }

    // 提取可选的 display_name 和 agent_type（用于 Dashboard 友好显示）
    // W2 修复：超长值返回明确错误而非静默截断，让调用方感知问题
    let display_name = args["display_name"].as_str();
    if let Some(name) = display_name {
        if name.len() > 128 {
            return DaemonResponse::err(
                "INVALID_ARGS",
                format!("display_name 超出长度限制（最大 128 字符，实际 {} 字符）", name.len()),
            );
        }
    }
    let agent_type = args["agent_type"].as_str();
    if let Some(atype) = agent_type {
        if atype.len() > 64 {
            return DaemonResponse::err(
                "INVALID_ARGS",
                format!("agent_type 超出长度限制（最大 64 字符，实际 {} 字符）", atype.len()),
            );
        }
    }

    match lifecycle::start_actor(state, group_id, actor_id, display_name, agent_type).await {
        Ok(session_state) => DaemonResponse::ok(serde_json::to_value(session_state).unwrap_or_default()),
        Err(e) => DaemonResponse::err("LIFECYCLE_ERROR", e.to_string()),
    }
}

/// actor_stop handler
///
/// 停止指定 Actor（幂等操作），移除 HeadlessSession
///
/// 必填参数：group_id, actor_id
///
/// @param state - 共享应用状态
/// @param args - 请求参数
async fn handle_actor_stop(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    // 提取必填参数
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };
    let actor_id = match args["actor_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: actor_id"),
    };
    if let Err(resp) = validate_id(group_id, "group_id") { return resp; }
    if let Err(resp) = validate_id(actor_id, "actor_id") { return resp; }

    match lifecycle::stop_actor(state, group_id, actor_id).await {
        Ok(()) => DaemonResponse::ok(serde_json::json!({ "stopped": true, "actor_id": actor_id })),
        Err(e) => DaemonResponse::err("LIFECYCLE_ERROR", e.to_string()),
    }
}

/// headless_status handler
///
/// 查询指定 Actor 的 Headless 运行状态
///
/// 必填参数：group_id, actor_id
///
/// @param state - 共享应用状态
/// @param args - 请求参数
async fn handle_headless_status(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    // 提取必填参数
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };
    let actor_id = match args["actor_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: actor_id"),
    };

    if let Err(resp) = validate_id(group_id, "group_id") { return resp; }
    if let Err(resp) = validate_id(actor_id, "actor_id") { return resp; }

    match lifecycle::get_headless_status(state, group_id, actor_id).await {
        Some(state_snapshot) => DaemonResponse::ok(serde_json::to_value(state_snapshot).unwrap_or_default()),
        None => DaemonResponse::err(
            "SESSION_NOT_FOUND",
            format!("no active session for actor '{}' in group '{}'", actor_id, group_id),
        ),
    }
}

/// headless_set_status handler
///
/// 更新指定 Actor 的 Headless 运行状态
///
/// 必填参数：group_id, actor_id, status
/// 可选参数：task_id
///
/// @param state - 共享应用状态
/// @param args - 请求参数
async fn handle_headless_set_status(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    // 提取必填参数
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };
    let actor_id = match args["actor_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: actor_id"),
    };
    let status_str = match args["status"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: status"),
    };

    // 解析 status 枚举（snake_case）
    let status = match status_str {
        "idle" => HeadlessStatus::Idle,
        "working" => HeadlessStatus::Working,
        "waiting" => HeadlessStatus::Waiting,
        "stopped" => HeadlessStatus::Stopped,
        other => {
            return DaemonResponse::err(
                "INVALID_ARGS",
                format!("invalid status '{}', must be one of: idle, working, waiting, stopped", other),
            );
        }
    };

    if let Err(resp) = validate_id(group_id, "group_id") { return resp; }
    if let Err(resp) = validate_id(actor_id, "actor_id") { return resp; }

    // 可选参数 task_id
    let task_id = args["task_id"].as_str().map(|s| s.to_string());

    match lifecycle::set_headless_status(state, group_id, actor_id, status, task_id).await {
        Ok(state_snapshot) => DaemonResponse::ok(serde_json::to_value(state_snapshot).unwrap_or_default()),
        Err(e) => DaemonResponse::err("LIFECYCLE_ERROR", e.to_string()),
    }
}

/// send handler
///
/// 发送消息到指定收件人或广播
///
/// 必填参数：group_id, sender_id (或 by), body (或 text)
/// 可选参数：to (收件人列表，空=广播), reply_to (回复的 event_id)
///
/// @param state - 共享应用状态
/// @param args - 请求参数
async fn handle_send(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    // 提取 group_id（必填）
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };
    // 提取 sender_id（必填，兼容 "by" 字段名）
    let sender_id = match args["sender_id"].as_str().or_else(|| args["by"].as_str()) {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: sender_id"),
    };
    // 提取 body（必填，兼容 "text" 字段名）
    let body = match args["body"].as_str().or_else(|| args["text"].as_str()) {
        Some(v) => v.to_string(),
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: body"),
    };
    // 提取 to（可选，默认空=广播）
    let recipients: Vec<String> = args["to"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    if let Err(resp) = validate_id(group_id, "group_id") { return resp; }

    // 提取 reply_to（可选）
    let reply_to = args["reply_to"].as_str().map(String::from);

    match send::send_message(state, group_id, sender_id, recipients, body, reply_to).await {
        Ok(event) => DaemonResponse::ok(serde_json::json!({
            "event": serde_json::to_value(&event).unwrap_or_default()
        })),
        Err(e) => DaemonResponse::err("MESSAGING_ERROR", e.to_string()),
    }
}

/// reply handler
///
/// 回复指定消息
///
/// 必填参数：group_id, sender_id (或 by), reply_to, body (或 text)
///
/// @param state - 共享应用状态
/// @param args - 请求参数
async fn handle_reply(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };
    let sender_id = match args["sender_id"].as_str().or_else(|| args["by"].as_str()) {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: sender_id"),
    };
    let reply_to = match args["reply_to"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: reply_to"),
    };
    let body = match args["body"].as_str().or_else(|| args["text"].as_str()) {
        Some(v) => v.to_string(),
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: body"),
    };

    if let Err(resp) = validate_id(group_id, "group_id") { return resp; }

    match send::reply_message(state, group_id, sender_id, reply_to, body).await {
        Ok(event) => DaemonResponse::ok(serde_json::json!({
            "event": serde_json::to_value(&event).unwrap_or_default()
        })),
        Err(e) => DaemonResponse::err("MESSAGING_ERROR", e.to_string()),
    }
}

/// inbox_list handler
///
/// 获取 Actor 的未读消息列表
///
/// 必填参数：group_id, actor_id
/// 可选参数：limit (默认 50)
///
/// @param state - 共享应用状态
/// @param args - 请求参数
async fn handle_inbox_list(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };
    let actor_id = match args["actor_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: actor_id"),
    };
    if let Err(resp) = validate_id(group_id, "group_id") { return resp; }
    if let Err(resp) = validate_id(actor_id, "actor_id") { return resp; }
    let limit = args["limit"].as_u64().unwrap_or(50) as usize;

    match inbox::unread_messages(state, group_id, actor_id, limit) {
        Ok(messages) => {
            // Agent 主动拉取消息后，清除 has_unread 标记
            // 避免 ping 持续返回 has_unread=true（误报）
            state.delivery.throttle.clear_unread(group_id, actor_id);

            DaemonResponse::ok(serde_json::json!({
                "messages": serde_json::to_value(&messages).unwrap_or_default(),
                "count": messages.len(),
            }))
        }
        Err(e) => DaemonResponse::err("MESSAGING_ERROR", e.to_string()),
    }
}

/// inbox_mark_read handler
///
/// 标记已读到指定事件
///
/// 必填参数：group_id, actor_id, event_id
///
/// @param state - 共享应用状态
/// @param args - 请求参数
async fn handle_inbox_mark_read(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };
    let actor_id = match args["actor_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: actor_id"),
    };
    let event_id = match args["event_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: event_id"),
    };

    if let Err(resp) = validate_id(group_id, "group_id") { return resp; }
    if let Err(resp) = validate_id(actor_id, "actor_id") { return resp; }

    match inbox::mark_read(state, group_id, actor_id, event_id) {
        Ok(()) => DaemonResponse::ok(serde_json::json!({ "marked": true })),
        Err(e) => DaemonResponse::err("MESSAGING_ERROR", e.to_string()),
    }
}

/// inbox_mark_all_read handler
///
/// 全部标记已读
///
/// 必填参数：group_id, actor_id
///
/// @param state - 共享应用状态
/// @param args - 请求参数
async fn handle_inbox_mark_all_read(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };
    let actor_id = match args["actor_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: actor_id"),
    };

    if let Err(resp) = validate_id(group_id, "group_id") { return resp; }
    if let Err(resp) = validate_id(actor_id, "actor_id") { return resp; }

    match inbox::mark_all_read(state, group_id, actor_id) {
        Ok(()) => DaemonResponse::ok(serde_json::json!({ "marked_all": true })),
        Err(e) => DaemonResponse::err("MESSAGING_ERROR", e.to_string()),
    }
}

/// 验证 ID 格式是否合法（防止路径遍历攻击）
///
/// 合法字符：字母、数字、连字符、下划线、点号
/// 禁止：空字符串、包含 / 或 .. 的值
fn validate_id(id: &str, field_name: &str) -> std::result::Result<(), DaemonResponse> {
    if id.is_empty() {
        return Err(DaemonResponse::err("INVALID_ARGS", format!("{} 不能为空", field_name)));
    }
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(DaemonResponse::err(
            "INVALID_ARGS",
            format!("{} 包含非法字符（不允许 /、\\、..）: \"{}\"", field_name, id),
        ));
    }
    if !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.') {
        return Err(DaemonResponse::err(
            "INVALID_ARGS",
            format!("{} 只允许字母、数字、连字符、下划线和点号: \"{}\"", field_name, id),
        ));
    }
    Ok(())
}

// ============================================
// 路由 Handler 实现（Phase 2）
// ============================================

/// route_task handler
///
/// 提交单个任务到指定后端，注册到路由状态表，返回 task_id。
///
/// 业务逻辑：
/// 1. 提取必填参数（group_id, task_text）和可选参数（backend, workdir）
/// 2. 验证 group_id 格式（防路径遍历）
/// 3. 通过 SovereigntyGuard 检查后端写入权限
/// 4. 生成 UUID task_id，注册到 RoutingState
/// 5. 返回 { task_id, backend, can_write }
///
/// 必填参数：group_id, task_text
/// 可选参数：backend（默认 "claude"）, workdir
///
/// @param state - 共享应用状态
/// @param args - 请求参数
async fn handle_route_task(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    // 提取必填参数
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };
    let task_text = match args["task_text"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: task_text"),
    };

    // 防路径遍历校验
    if let Err(resp) = validate_id(group_id, "group_id") {
        return resp;
    }

    // 可选参数
    let backend_name = args["backend"].as_str().unwrap_or("claude");
    let workdir = args["workdir"]
        .as_str()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| state.groups_dir.clone());
    let actor_id = args["actor_id"].as_str().unwrap_or("default");
    // cli_path_override: 测试专用参数，允许指定 CLI 可执行文件的完整路径
    // 生产环境不传此参数，使用 PATH 查找 CLI
    let cli_path_override = args["_cli_path"].as_str();

    // 检查后端写入权限（代码主权守卫）
    let can_write = state.routing.sovereignty.can_write(backend_name);

    // 生成 UUID task_id
    let task_id = Uuid::new_v4().to_string();

    // 注册任务到路由状态表（初始状态为 pending）
    state.routing.register_task(group_id, &task_id, backend_name).await;

    // ============================================
    // 构造 ExecutableTask 并异步后台执行
    // 立即返回 task_id 给调用方，执行结果通过 route_status 查询
    // ============================================
    let executable_task = build_single_executable_task(
        state,
        &task_id,
        task_text,
        backend_name,
        actor_id,
        group_id,
        &workdir,
        cli_path_override,
    );

    // 克隆所需 Arc 给后台任务
    let routing = Arc::clone(&state.routing);
    let session_store = Arc::clone(&state.session_store);
    let group_id_owned = group_id.to_string();
    let task_id_owned = task_id.clone();
    let actor_id_owned = actor_id.to_string();
    let backend_name_owned = backend_name.to_string();

    // 后台执行，不阻塞当前请求
    tokio::spawn(async move {
        execute_single_task_and_update_state(
            executable_task,
            routing,
            session_store,
            &group_id_owned,
            &task_id_owned,
            &actor_id_owned,
            &backend_name_owned,
        )
        .await;
    });

    DaemonResponse::ok(serde_json::json!({
        "task_id": task_id,
        "backend": backend_name,
        "can_write": can_write,
        "group_id": group_id,
        "task_text": task_text,
    }))
}

/// 构造单任务的 ExecutableTask
///
/// 业务逻辑：
/// 1. 检查 SessionStore 是否有已有 session_id -> Resume/New 模式
/// 2. 根据 backend 名称选择对应的 Backend 实现
/// 3. 构造 TaskConfig（含 workdir、mode、session_id）
/// 4. 使用 Backend::build_args 构建命令行参数
/// 5. 根据 should_use_stdin 决定任务文本传递方式
///
/// @param state - 共享应用状态
/// @param task_id - 任务唯一 ID
/// @param task_text - 任务描述文本
/// @param backend_name - 后端名称 (claude/codex/gemini)
/// @param actor_id - Actor ID，用于 SessionStore key
/// @param group_id - Group ID，用于 SessionStore key
/// @param workdir - 工作目录
/// @param cli_path_override - 可选的 CLI 可执行文件路径覆盖（测试专用）
fn build_single_executable_task(
    state: &AppState,
    task_id: &str,
    task_text: &str,
    backend_name: &str,
    actor_id: &str,
    group_id: &str,
    workdir: &std::path::PathBuf,
    cli_path_override: Option<&str>,
) -> ExecutableTask {
    // ============================================
    // 第一步：检查 SessionStore，确定 Resume/New 模式
    // ============================================
    let session_key: SessionKey = (
        group_id.to_string(),
        actor_id.to_string(),
        backend_name.to_string(),
    );
    let existing_session_id = state.session_store.get(&session_key);
    let (mode, session_id) = if let Some(sid) = existing_session_id {
        (TaskMode::Resume, Some(sid))
    } else {
        (TaskMode::New, None)
    };

    // ============================================
    // 第二步：构造 TaskConfig
    // ============================================
    let task_config = TaskConfig {
        workdir: workdir.clone(),
        mode,
        session_id,
        model: None,
        // 任务超时：30 秒（集成测试中 fake CLI 立即返回，不会超时）
        timeout: Duration::from_secs(30),
    };

    // ============================================
    // 第三步：根据 backend_name 选择 Backend 实现并构建参数
    // cli_path_override 允许在测试中注入 fake CLI 的完整路径
    // ============================================
    let (command, mut args) = match backend_name {
        "codex" => {
            let b = CodexBackend;
            let cmd = cli_path_override.unwrap_or_else(|| b.command()).to_string();
            (cmd, b.build_args(&task_config))
        }
        "gemini" => {
            let b = GeminiBackend::new(None);
            let cmd = cli_path_override.unwrap_or_else(|| b.command()).to_string();
            (cmd, b.build_args(&task_config))
        }
        _ => {
            // 默认使用 claude
            let b = ClaudeBackend;
            let cmd = cli_path_override.unwrap_or_else(|| b.command()).to_string();
            (cmd, b.build_args(&task_config))
        }
    };

    // ============================================
    // 第四步：决定任务文本传递方式
    // 短文本：追加为命令行最后一个参数
    // 长文本或含特殊字符：通过 stdin 传递
    // ============================================
    let stdin_data = if should_use_stdin(task_text) {
        // stdin 模式：不追加到 args
        Some(task_text.to_string())
    } else {
        // 参数模式：追加到 args 末尾
        args.push(task_text.to_string());
        None
    };

    ExecutableTask {
        id: task_id.to_string(),
        command,
        args,
        stdin_data,
        dependencies: vec![],
        timeout: task_config.timeout,
    }
}

/// 执行单任务并更新路由状态和 SessionStore
///
/// 业务逻辑：
/// 1. 调用 Executor::run 执行任务
/// 2. 解析 stdout 提取 session_id（使用 StreamParser）
/// 3. 根据执行结果更新 RoutingState（completed/failed）
/// 4. 如果成功且有 session_id，写入 SessionStore
///
/// @param task - 可执行任务
/// @param routing - 路由状态管理器
/// @param session_store - Session 持久化存储
/// @param group_id - Group ID
/// @param task_id - 任务 ID
/// @param actor_id - Actor ID
/// @param backend_name - 后端名称
async fn execute_single_task_and_update_state(
    task: ExecutableTask,
    routing: Arc<RoutingState>,
    session_store: Arc<ghostcode_router::session::SessionStore>,
    group_id: &str,
    task_id: &str,
    actor_id: &str,
    backend_name: &str,
) {
    let cancel = CancellationToken::new();
    let config = ExecutorConfig {
        max_workers: 1,
        cancel,
    };

    // 更新状态为 running
    routing.update_task(group_id, task_id, "running", None).await;

    let results = Executor::run(vec![task], config).await;

    // 取出第一个（也是唯一一个）任务的结果
    let result = match results.into_iter().next() {
        Some(r) => r,
        None => {
            routing.update_task(group_id, task_id, "failed", Some("执行器返回空结果".to_string())).await;
            return;
        }
    };

    match &result.status {
        TaskStatus::Success => {
            // ============================================
            // 执行成功：提取 session_id 并写入 SessionStore
            // ============================================
            let session_id = extract_session_id(&result.output);

            if let Some(sid) = session_id {
                let session_key: SessionKey = (
                    group_id.to_string(),
                    actor_id.to_string(),
                    backend_name.to_string(),
                );
                // session_id 写入失败不影响任务状态更新，仅记录警告
                if let Err(e) = session_store.save(session_key, sid) {
                    tracing::warn!("session_id 写入 SessionStore 失败: {}", e);
                }
            }

            let output_text = result
                .output
                .as_ref()
                .map(|o| o.stdout_lines.join("\n"))
                .unwrap_or_default();
            routing.update_task(group_id, task_id, "completed", Some(output_text)).await;
        }
        TaskStatus::Failed(err_msg) => {
            routing.update_task(group_id, task_id, "failed", Some(err_msg.clone())).await;
        }
        TaskStatus::Skipped(reason) => {
            routing.update_task(group_id, task_id, "failed", Some(format!("任务被跳过: {}", reason))).await;
        }
        TaskStatus::Cancelled => {
            routing.update_task(group_id, task_id, "failed", Some("任务被取消".to_string())).await;
        }
    }
}

/// 从进程输出中提取 session_id
///
/// 使用 StreamParser 逐行解析 stdout，提取第一个锁定的 session_id。
/// 支持 Claude/Codex/Gemini 三种输出格式。
///
/// @param output - 进程输出（可能为 None）
/// @returns 提取到的 session_id，如果无法提取则返回 None
fn extract_session_id(output: &Option<ghostcode_router::process::ProcessOutput>) -> Option<String> {
    let output = output.as_ref()?;

    let mut parser = StreamParser::new();
    for line in &output.stdout_lines {
        let _ = parser.parse_line(line);
    }

    parser.session_id().map(|s| s.to_string())
}

/// 将单个 TaskResult 映射为 RoutingState 状态字符串（Refactor 抽出的纯函数）
///
/// @param status - Executor 的 TaskStatus
/// @returns RoutingState 中使用的状态字符串
fn task_status_to_routing_status(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Success => "completed",
        TaskStatus::Failed(_) => "failed",
        TaskStatus::Skipped(_) => "failed",
        TaskStatus::Cancelled => "failed",
    }
}


/// 执行并行任务集并更新路由状态
///
/// 业务逻辑：
/// 1. 调用 Executor::run 执行所有任务（DAG 层间串行+层内并行）
/// 2. 遍历结果，更新每个子任务的 RoutingState（completed/failed）
/// 3. 汇总子任务状态，调用 update_parallel_group_result 更新组合任务
///
/// @param tasks - 可执行任务列表（含依赖关系）
/// @param routing - 路由状态管理器
/// @param session_store - Session 持久化存储
/// @param group_id - Group ID
/// @param group_task_id - 组合任务 ID（route_status 查询此 ID 获取汇总状态）
async fn execute_parallel_tasks_and_update_state(
    tasks: Vec<ExecutableTask>,
    routing: Arc<RoutingState>,
    session_store: Arc<ghostcode_router::session::SessionStore>,
    group_id: &str,
    group_task_id: &str,
) {
    let cancel = CancellationToken::new();
    let config = ExecutorConfig {
        max_workers: 4,
        cancel,
    };

    // 更新组合任务状态为 running
    routing.update_task(group_id, group_task_id, "running", None).await;

    let results = Executor::run(tasks, config).await;

    // ============================================
    // 更新每个子任务的 RoutingState
    // ============================================
    let mut subtask_summaries: Vec<crate::routing::SubtaskSummary> = Vec::with_capacity(results.len());

    for result in &results {
        let final_status = task_status_to_routing_status(&result.status);
        let result_text = match &result.status {
            TaskStatus::Success => result
                .output
                .as_ref()
                .map(|o| o.stdout_lines.join("\n")),
            TaskStatus::Failed(msg) => Some(msg.clone()),
            TaskStatus::Skipped(reason) => Some(format!("跳过: {}", reason)),
            TaskStatus::Cancelled => Some("已取消".to_string()),
        };

        // 更新子任务状态（子任务以 TaskSpec.id 注册，不是 UUID）
        // 注意：子任务没有在 RoutingState 中预注册，此处使用 update_task 时可能无效
        // 实际上子任务不需要单独注册——汇总状态通过 group_task_id 的 subtasks 字段返回
        routing
            .update_task(group_id, &result.id, final_status, result_text)
            .await;

        subtask_summaries.push(crate::routing::SubtaskSummary {
            id: result.id.clone(),
            status: final_status.to_string(),
        });

        // 成功执行时，提取 session_id 写入 SessionStore
        if matches!(result.status, TaskStatus::Success) {
            if let Some(sid) = extract_session_id(&result.output) {
                let session_key: SessionKey = (
                    group_id.to_string(),
                    "default".to_string(),
                    "parallel".to_string(),
                );
                if let Err(e) = session_store.save(session_key, sid) {
                    tracing::warn!("并行任务 {} session_id 写入失败: {}", result.id, e);
                }
            }
        }
    }

    // ============================================
    // 汇总子任务状态，更新组合任务
    // ============================================
    routing
        .update_parallel_group_result(group_id, group_task_id, subtask_summaries)
        .await;
}

/// route_task_parallel handler
///
/// 提交并行任务集合（---TASK---/---CONTENT--- 格式），验证 DAG 依赖，注册到路由状态表。
///
/// 业务逻辑：
/// 1. 提取必填参数（group_id, tasks_format）
/// 2. 验证 group_id 格式
/// 3. 用 parse_task_format() 解析任务格式文本
/// 4. 用 topological_sort() 验证 DAG，检测循环依赖
/// 5. 生成并行任务组 task_id，注册到 RoutingState
/// 6. 返回 { task_id, task_count, layers }
///
/// 必填参数：group_id, tasks_format
///
/// @param state - 共享应用状态
/// @param args - 请求参数
async fn handle_route_task_parallel(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    // 提取必填参数
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };
    let tasks_format = match args["tasks_format"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: tasks_format"),
    };

    // 防路径遍历校验
    if let Err(resp) = validate_id(group_id, "group_id") {
        return resp;
    }

    // 测试专用参数：CLI 路径覆盖（统一应用于所有子任务）
    let cli_path_override = args["_cli_path"].as_str();

    // 解析任务格式文本（---TASK---/---CONTENT--- 格式）
    let specs = match parse_task_format(tasks_format) {
        Ok(s) => s,
        Err(e) => return DaemonResponse::err("INVALID_ARGS", format!("任务格式解析失败: {}", e)),
    };

    let task_count = specs.len();

    // 构造 DAG 节点，验证依赖关系
    let nodes: Vec<TaskNode> = specs
        .iter()
        .map(|s| TaskNode {
            id: s.id.clone(),
            dependencies: s.dependencies.clone(),
        })
        .collect();

    // 拓扑排序验证（检测循环依赖和缺失依赖）
    let layers = match topological_sort(nodes) {
        Ok(l) => l,
        Err(e) => return DaemonResponse::err("INVALID_ARGS", format!("DAG 验证失败: {}", e)),
    };

    let layer_count = layers.len();

    // 生成并行任务组的 UUID task_id，并注册为 pending（backend="parallel"）
    let group_task_id = Uuid::new_v4().to_string();
    state.routing.register_task(group_id, &group_task_id, "parallel").await;

    // ============================================
    // 构造所有子任务的 ExecutableTask 列表
    // 复用 build_single_executable_task 保持一致性
    // ============================================
    let workdir = state.groups_dir.clone();
    let mut executable_tasks: Vec<ExecutableTask> = Vec::with_capacity(specs.len());

    for spec in &specs {
        let actor_id = "default"; // 并行任务暂用默认 actor_id
        let task = build_single_executable_task(
            state,
            &spec.id,
            &spec.task_text,
            &spec.backend,
            actor_id,
            group_id,
            &workdir,
            cli_path_override,
        );
        // 将 TaskSpec 的依赖关系传入 ExecutableTask
        let task_with_deps = ExecutableTask {
            dependencies: spec.dependencies.clone(),
            ..task
        };
        executable_tasks.push(task_with_deps);
    }

    // 克隆所需 Arc 给后台任务
    let routing = Arc::clone(&state.routing);
    let session_store = Arc::clone(&state.session_store);
    let group_id_owned = group_id.to_string();
    let group_task_id_owned = group_task_id.clone();

    // 后台执行并行任务，不阻塞当前请求
    tokio::spawn(async move {
        execute_parallel_tasks_and_update_state(
            executable_tasks,
            routing,
            session_store,
            &group_id_owned,
            &group_task_id_owned,
        )
        .await;
    });

    DaemonResponse::ok(serde_json::json!({
        "task_id": group_task_id,
        "task_count": task_count,
        "layers": layer_count,
        "group_id": group_id,
    }))
}

/// route_status handler
///
/// 查询指定路由任务的当前状态。
///
/// 业务逻辑：
/// 1. 提取必填参数（group_id, task_id）
/// 2. 从 RoutingState 查询任务状态
/// 3. 存在则返回 RouteTaskState，不存在则返回 NOT_FOUND 错误
///
/// 必填参数：group_id, task_id
///
/// @param state - 共享应用状态
/// @param args - 请求参数
async fn handle_route_status(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    // 提取必填参数
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };
    let task_id = match args["task_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: task_id"),
    };

    if let Err(resp) = validate_id(group_id, "group_id") {
        return resp;
    }

    match state.routing.get_task(group_id, task_id).await {
        Some(task_state) => DaemonResponse::ok(
            serde_json::to_value(task_state).unwrap_or_default(),
        ),
        None => DaemonResponse::err(
            "NOT_FOUND",
            format!("路由任务 '{}' 不存在", task_id),
        ),
    }
}

/// route_cancel handler
///
/// 取消指定路由任务（标记为 cancelled），幂等操作。
///
/// 业务逻辑：
/// 1. 提取必填参数（group_id, task_id）
/// 2. 调用 RoutingState::cancel_task()，幂等取消
/// 3. 始终返回 { cancelled: true }（任务存在或已取消均视为成功）
///
/// 必填参数：group_id, task_id
///
/// @param state - 共享应用状态
/// @param args - 请求参数
async fn handle_route_cancel(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    // 提取必填参数
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };
    let task_id = match args["task_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: task_id"),
    };

    if let Err(resp) = validate_id(group_id, "group_id") {
        return resp;
    }

    // 取消任务（幂等：不存在时 cancel_task 返回 false，但仍视为成功）
    state.routing.cancel_task(group_id, task_id).await;

    DaemonResponse::ok(serde_json::json!({
        "cancelled": true,
        "task_id": task_id,
    }))
}

/// session_list handler
///
/// 列出指定 Group 的所有 Agent 会话。
/// 列出指定 Group 的所有 AI 后端会话记录
///
/// Phase 6 Task 4：从 AppState.session_store 读取真实 session 数据，
/// 按 group_id 过滤，映射为响应数组并按 (actor_id, backend) 升序排序。
///
/// 业务逻辑：
/// 1. 验证 group_id 必填参数（防路径遍历）
/// 2. 调用 state.session_store.list() 获取所有 session 条目
/// 3. 按 group_id 过滤，只返回属于该 group 的记录
/// 4. 映射为 { actor_id, backend, session_id } 结构体
/// 5. 按 (actor_id, backend) 升序排序，保证响应顺序确定性
/// 6. 返回 { group_id, sessions } 响应
///
/// 必填参数：group_id
///
/// @param state - 共享应用状态（含 session_store）
/// @param args - 请求参数
async fn handle_session_list(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    // 提取并验证必填参数 group_id
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };

    if let Err(resp) = validate_id(group_id, "group_id") {
        return resp;
    }

    // 从 session_store 读取所有 session 条目，按 group_id 过滤
    // SessionStore::list() 返回 Vec<(SessionKey, String)>
    // SessionKey 为 (group_id, actor_id, backend) 三元组
    let all_sessions = state.session_store.list();

    // 过滤出属于指定 group 的 session 条目，映射为响应结构
    let mut sessions: Vec<serde_json::Value> = all_sessions
        .into_iter()
        .filter(|((gid, _, _), _)| gid == group_id)
        .map(|((_, actor_id, backend), session_id)| {
            serde_json::json!({
                "actor_id": actor_id,
                "backend": backend,
                "session_id": session_id,
            })
        })
        .collect();

    // 按 (actor_id, backend) 升序排序，保证响应顺序确定性
    sessions.sort_by(|a, b| {
        let a_actor = a["actor_id"].as_str().unwrap_or("");
        let b_actor = b["actor_id"].as_str().unwrap_or("");
        let actor_cmp = a_actor.cmp(b_actor);
        if actor_cmp != std::cmp::Ordering::Equal {
            return actor_cmp;
        }
        let a_backend = a["backend"].as_str().unwrap_or("");
        let b_backend = b["backend"].as_str().unwrap_or("");
        a_backend.cmp(b_backend)
    });

    DaemonResponse::ok(serde_json::json!({
        "group_id": group_id,
        "sessions": sessions,
    }))
}

// ============================================
// 验证 Handler（Phase 3 脚手架）
// ============================================

/// verification_start handler
///
/// 启动 Ralph 验证循环，创建新运行记录。
///
/// 业务逻辑：
/// 1. 提取必填参数 group_id、run_id，缺失返回 INVALID_ARGS
/// 2. 验证 ID 格式合法性（防路径遍历）
/// 3. 加锁调用 start_run 创建新运行（已存在则返回 VERIFICATION_ERROR）
/// 4. clone RunState 后释放锁，序列化返回
///
/// 必填参数：group_id, run_id
///
/// @param state - 共享应用状态
/// @param args - 请求参数
fn handle_verification_start(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };
    let run_id = match args["run_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: run_id"),
    };
    if let Err(resp) = validate_id(group_id, "group_id") { return resp; }
    if let Err(resp) = validate_id(run_id, "run_id") { return resp; }

    let mut store = state.verification.lock().unwrap_or_else(|e| e.into_inner());
    match store.start_run(group_id.to_string(), run_id.to_string()) {
        Ok(()) => {
            // 获取刚创建的 RunState 并 clone，避免持有借用跨越 drop
            let run_state = store.get_run(group_id, run_id).cloned();
            drop(store);
            match run_state {
                Some(s) => DaemonResponse::ok(serde_json::to_value(s).unwrap_or_default()),
                None => DaemonResponse::ok(serde_json::json!({ "started": true })),
            }
        }
        Err(e) => DaemonResponse::err("VERIFICATION_ERROR", e.to_string()),
    }
}

/// verification_status handler
///
/// 查询指定验证运行的当前状态。
///
/// 业务逻辑：
/// 1. 提取必填参数 group_id、run_id，缺失返回 INVALID_ARGS
/// 2. 验证 ID 格式合法性
/// 3. 加锁查询 get_run，不存在返回 NOT_FOUND
/// 4. clone RunState 后释放锁，序列化返回
///
/// 必填参数：group_id, run_id
///
/// @param state - 共享应用状态
/// @param args - 请求参数
fn handle_verification_status(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };
    let run_id = match args["run_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: run_id"),
    };
    if let Err(resp) = validate_id(group_id, "group_id") { return resp; }
    if let Err(resp) = validate_id(run_id, "run_id") { return resp; }

    let store = state.verification.lock().unwrap_or_else(|e| e.into_inner());
    match store.get_run(group_id, run_id) {
        // clone RunState，确保可以安全释放锁后序列化
        Some(s) => DaemonResponse::ok(serde_json::to_value(s.clone()).unwrap_or_default()),
        None => DaemonResponse::err(
            "NOT_FOUND",
            format!("验证运行 ({}, {}) 不存在", group_id, run_id),
        ),
    }
}

/// verification_cancel handler
///
/// 取消进行中的验证运行，将状态推进到 Cancelled 终态。
///
/// 业务逻辑：
/// 1. 提取必填参数 group_id、run_id，缺失返回 INVALID_ARGS
/// 2. 验证 ID 格式合法性
/// 3. 加锁调用 apply_event(Cancel) 进行状态迁移
/// 4. 成功返回 { cancelled: true }
///
/// 必填参数：group_id, run_id
///
/// @param state - 共享应用状态
/// @param args - 请求参数
fn handle_verification_cancel(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };
    let run_id = match args["run_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: run_id"),
    };
    if let Err(resp) = validate_id(group_id, "group_id") { return resp; }
    if let Err(resp) = validate_id(run_id, "run_id") { return resp; }

    let mut store = state.verification.lock().unwrap_or_else(|e| e.into_inner());
    match store.apply_event(group_id, run_id, VerifyEvent::Cancel) {
        Ok(()) => DaemonResponse::ok(serde_json::json!({ "cancelled": true })),
        // 运行不存在：与 verification_status 保持一致，返回 NOT_FOUND
        Err(crate::verification::VerifyError::RunNotFound { .. }) => DaemonResponse::err(
            "NOT_FOUND",
            format!("验证运行 ({}, {}) 不存在", group_id, run_id),
        ),
        // 终态下取消视为幂等成功（cancel 语义：确保运行不再进行）
        // Running 状态下 Cancel 一定成功，所以 IllegalOperation 只会来自终态
        Err(crate::verification::VerifyError::IllegalOperation { .. }) => {
            DaemonResponse::ok(serde_json::json!({ "cancelled": true }))
        }
        Err(e) => DaemonResponse::err("VERIFICATION_ERROR", e.to_string()),
    }
}

// ============================================
// Skill Learning Handler（Phase 4）
// ============================================

/// handle_skill_learn_fragment: 摄入会话片段，返回候选（若创建）
///
/// C4 修复：新增 group_id 必填参数，以 group_id 为 key 隔离存储
///
/// 必填参数：group_id, problem, solution
/// 可选参数：confidence, context, suggested_triggers, suggested_tags
///
/// @param state - 共享应用状态
/// @param args - 请求参数
fn handle_skill_learn_fragment(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    // C4 修复：group_id 作为必填参数，实现 skill 数据按 group 隔离
    let group_id = match args["group_id"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return DaemonResponse::err("INVALID_ARGS", "缺少 group_id 字段"),
    };
    if let Err(resp) = validate_id(&group_id, "group_id") { return resp; }

    let problem = match args["problem"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return DaemonResponse::err("INVALID_ARGS", "缺少 problem 字段"),
    };
    let solution = match args["solution"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return DaemonResponse::err("INVALID_ARGS", "缺少 solution 字段"),
    };
    let confidence = args["confidence"].as_u64().unwrap_or(0) as u8;
    let context = args["context"].as_str().unwrap_or("").to_string();
    let suggested_triggers: Vec<String> = args["suggested_triggers"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let suggested_tags: Vec<String> = args["suggested_tags"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    use crate::skill_learning::{ingest_session_fragment, SessionFragment, SkillStore};
    let fragment = SessionFragment {
        problem,
        solution,
        confidence,
        context,
        suggested_triggers,
        suggested_tags,
    };
    // C4 修复：通过 group_id 访问对应 group 的 SkillStore，不存在时自动创建
    let mut store_map = state.skill_store.lock().unwrap_or_else(|e| e.into_inner());
    let group_store = store_map.entry(group_id).or_insert_with(SkillStore::new);
    let result = ingest_session_fragment(group_store, fragment);

    DaemonResponse::ok(serde_json::to_value(result).unwrap_or(serde_json::Value::Null))
}

/// handle_skill_list: 列出指定 Group 的所有候选
///
/// C4 修复：新增 group_id 必填参数，只返回对应 group 的候选
///
/// 必填参数：group_id
///
/// @param state - 共享应用状态
/// @param args - 请求参数
fn handle_skill_list(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    // C4 修复：group_id 作为必填参数，实现 skill 数据按 group 隔离
    let group_id = match args["group_id"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return DaemonResponse::err("INVALID_ARGS", "缺少 group_id 字段"),
    };
    if let Err(resp) = validate_id(&group_id, "group_id") { return resp; }

    use crate::skill_learning::list_skill_candidates;
    let store_map = state.skill_store.lock().unwrap_or_else(|e| e.into_inner());
    // group 不存在时返回空列表
    let candidates = match store_map.get(&group_id) {
        Some(group_store) => list_skill_candidates(group_store),
        None => vec![],
    };
    DaemonResponse::ok(serde_json::to_value(candidates).unwrap_or(serde_json::Value::Null))
}

/// handle_skill_promote: 将候选提升为正式 Skill
///
/// C4 修复：新增 group_id 必填参数，只操作对应 group 的候选
/// W6 修复：promote 成功后添加 TODO 注释说明需要后续写账本事件
///
/// 必填参数：group_id, candidate_id, skill_id
/// 可选参数：skill_name（默认使用 skill_id）
///
/// @param state - 共享应用状态
/// @param args - 请求参数
fn handle_skill_promote(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    // C4 修复：group_id 作为必填参数，实现 skill 数据按 group 隔离
    let group_id = match args["group_id"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return DaemonResponse::err("INVALID_ARGS", "缺少 group_id 字段"),
    };
    if let Err(resp) = validate_id(&group_id, "group_id") { return resp; }

    let candidate_id = match args["candidate_id"].as_str() {
        Some(s) => s.to_string(),
        None => return DaemonResponse::err("INVALID_ARGS", "缺少 candidate_id"),
    };
    let skill_id = match args["skill_id"].as_str() {
        Some(s) => s.to_string(),
        None => return DaemonResponse::err("INVALID_ARGS", "缺少 skill_id"),
    };
    let skill_name = args["skill_name"]
        .as_str()
        .unwrap_or(&skill_id)
        .to_string();

    use crate::skill_learning::{promote_skill, SkillStore};
    let mut store_map = state.skill_store.lock().unwrap_or_else(|e| e.into_inner());
    // group 不存在时返回 NOT_FOUND（promote 要求候选已存在）
    let group_store = store_map.entry(group_id.clone()).or_insert_with(SkillStore::new);
    match promote_skill(group_store, &candidate_id, &skill_id, &skill_name) {
        Ok(skill) => {
            // W6 修复：skill_promote 成功后写 SkillPromoted 事件到账本
            // 通过 groups_dir + group_id 构造账本路径，使用 ghostcode-ledger::append_event
            // 写账本是尽力而为（best-effort）：失败不阻断 promote 响应
            let group_dir = state.groups_dir.join(&group_id);
            let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
            let lock_path = group_dir.join("state/ledger/ledger.lock");

            // 仅在账本目录存在时写入，避免路径不存在时触发 IO 错误
            if ledger_path.parent().map(|p| p.exists()).unwrap_or(false) {
                use ghostcode_ledger::append_event;
                use ghostcode_types::event::{Event, EventKind};

                let event = Event::new(
                    EventKind::SkillPromoted,
                    &group_id,
                    "default",
                    "user",
                    serde_json::json!({
                        "candidate_id": candidate_id,
                        "skill_id": skill_id,
                        "skill_name": skill_name,
                    }),
                );
                // 写入失败只记录警告，不回滚内存中的 promote 操作
                if let Err(e) = append_event(&ledger_path, &lock_path, &event) {
                    tracing::warn!("skill_promote 写账本事件失败（尽力而为）: {}", e);
                }
            }

            DaemonResponse::ok(serde_json::to_value(skill).unwrap_or(serde_json::Value::Null))
        }
        Err(msg) => DaemonResponse::err("NOT_FOUND", msg),
    }
}

/// handle_skill_extract: 从 problem/solution 文本中启发式提取 Skill 候选
///
/// Phase 6 Task 5：将 skill_extract 从 stub 升级为可用实现。
/// 不依赖 LLM 调用链，使用纯启发式规则（extract_skill_fragment）提取片段，
/// 再通过 ingest_session_fragment 入库（质量门 confidence >= 70 才创建候选）。
///
/// 业务逻辑：
/// 1. 验证 group_id 必填参数
/// 2. 读取 problem / solution 参数（均允许为空，但空值会触发低信号路径）
/// 3. 调用 extract_skill_fragment 判断信号质量并构建 SessionFragment
/// 4. 低信号（None）直接返回 ok + null result，无副作用
/// 5. 高信号则调用 ingest_session_fragment 入库，返回候选数据
///
/// 必填参数：group_id
/// 可选参数：problem, solution（均为字符串，允许为空）
///
/// @param state - 共享应用状态（含 skill_store）
/// @param args - 请求参数
fn handle_skill_extract(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    // 验证 group_id 必填参数，实现 group 级别数据隔离
    let group_id = match args["group_id"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return DaemonResponse::err("INVALID_ARGS", "缺少 group_id 字段"),
    };
    if let Err(resp) = validate_id(&group_id, "group_id") {
        return resp;
    }

    // 读取 problem 和 solution，允许为空（空值由 extract_skill_fragment 处理为低信号）
    let problem = args["problem"].as_str().unwrap_or("");
    let solution = args["solution"].as_str().unwrap_or("");

    use crate::skill_learning::{extract_skill_fragment, ingest_session_fragment, SkillStore};

    // 启发式提取：判断信号质量并构建 SessionFragment
    let fragment = match extract_skill_fragment(problem, solution) {
        Some(f) => f,
        // 低信号（均为空）：直接返回 null result，不写入存储
        None => return DaemonResponse::ok(serde_json::Value::Null),
    };

    // 通过 group_id 访问对应 group 的 SkillStore，不存在时自动创建
    let mut store_map = state.skill_store.lock().unwrap_or_else(|e| e.into_inner());
    let group_store = store_map.entry(group_id).or_insert_with(SkillStore::new);

    // 入库（质量门 confidence >= 70 才创建候选，低信号片段被 ingest 内部丢弃）
    let result = ingest_session_fragment(group_store, fragment);

    DaemonResponse::ok(serde_json::to_value(result).unwrap_or(serde_json::Value::Null))
}

// ============================================
// team_skill_list Handler（P9-T2）
// ============================================

/// 计算 (problem, solution) 的去重 key（与 skill_learning 模块保持一致的算法）
///
/// 使用相同的 DefaultHasher + problem/solution 组合 hash，
/// 确保跨 group 的相同技能可以正确识别并去重。
///
/// @param problem - 问题描述
/// @param solution - 解决方案
/// @returns 十六进制 hash 字符串
fn skill_dedup_key(problem: &str, solution: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    problem.hash(&mut hasher);
    solution.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// handle_team_skill_list: 聚合所有 group 的候选技能并返回排序结果
///
/// 业务逻辑：
/// 1. 遍历所有 group 的 skill_store，收集所有候选技能
/// 2. 按 (problem, solution) 的 hash 去重，相同内容只保留最高 confidence 版本
/// 3. 合并 source_groups（记录该技能来源于哪些 group）和 total_occurrences（累加）
/// 4. 按 confidence 降序、total_occurrences 降序排列
/// 5. 应用可选过滤参数：min_confidence（最低置信度）、limit（返回数量上限）
///
/// 可选参数：
/// - min_confidence: f64，只返回 confidence >= 此值的技能
/// - limit: usize，限制返回条数（取排序后前 N 条）
///
/// @param state - 共享应用状态（含 skill_store 按 group_id 隔离的技能数据）
/// @param args - 请求参数（min_confidence / limit 均为可选）
fn handle_team_skill_list(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    use std::collections::HashMap;
    use ghostcode_types::skill::TeamSkillSummary;
    use crate::skill_learning::list_skill_candidates;

    // ============================================
    // 第一步：解析可选过滤参数
    // ============================================
    let min_confidence: Option<f64> = args["min_confidence"].as_f64();
    let limit: Option<usize> = args["limit"].as_u64().map(|v| v as usize);

    // ============================================
    // 第二步：遍历所有 group，收集候选技能并聚合
    //
    // 使用 HashMap<dedup_key, TeamSkillSummary> 实现去重：
    // - key: (problem, solution) 的 hash
    // - value: 当前最高 confidence 版本的摘要
    // ============================================
    // 中间聚合结构：dedup_key -> (summary, occurrences_map)
    // occurrences_map 用于记录各 group 的 occurrences 累加
    let mut aggregated: HashMap<String, TeamSkillSummary> = HashMap::new();

    {
        let store_map = state.skill_store.lock().unwrap_or_else(|e| e.into_inner());
        for (group_id, group_store) in store_map.iter() {
            let candidates = list_skill_candidates(group_store);
            for candidate in candidates {
                let key = skill_dedup_key(&candidate.problem, &candidate.solution);

                if let Some(existing) = aggregated.get_mut(&key) {
                    // 已存在相同内容的技能：合并来源 group 和 occurrences
                    if !existing.source_groups.contains(group_id) {
                        existing.source_groups.push(group_id.clone());
                    }
                    // 累加跨 group 的出现次数
                    existing.total_occurrences += candidate.occurrences;
                    // 取最高 confidence（不降级）
                    let candidate_confidence = candidate.confidence as f64;
                    if candidate_confidence > existing.confidence {
                        existing.confidence = candidate_confidence;
                        // 更新描述为最高 confidence 版本的 solution
                        existing.description = Some(candidate.solution.clone());
                    }
                } else {
                    // 新技能：创建摘要条目
                    aggregated.insert(key.clone(), TeamSkillSummary {
                        name: candidate.problem.clone(),
                        source_groups: vec![group_id.clone()],
                        confidence: candidate.confidence as f64,
                        total_occurrences: candidate.occurrences,
                        description: Some(candidate.solution.clone()),
                        dedup_key: key,
                    });
                }
            }
        }
    }

    // ============================================
    // 第三步：排序 — confidence 降序，occurrences 降序
    // ============================================
    let mut result: Vec<TeamSkillSummary> = aggregated.into_values().collect();
    result.sort_by(|a, b| {
        // 先按 confidence 降序
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            // 再按 total_occurrences 降序（confidence 相同时）
            .then(b.total_occurrences.cmp(&a.total_occurrences))
    });

    // ============================================
    // 第四步：应用可选过滤
    // ============================================

    // min_confidence 过滤：只保留 confidence >= min_confidence 的技能
    if let Some(min_conf) = min_confidence {
        result.retain(|s| s.confidence >= min_conf);
    }

    // limit 过滤：截取前 N 条（排序后取头部）
    if let Some(lim) = limit {
        result.truncate(lim);
    }

    DaemonResponse::ok(serde_json::to_value(result).unwrap_or(serde_json::Value::Array(vec![])))
}

// ============================================
// HUD Handler（Phase 3 脚手架）
// ============================================

/// hud_snapshot handler
///
/// 获取 HUD 状态栏的当前快照。
///
/// 业务逻辑：
/// 1. 调用 build_hud_snapshot 同步聚合所有状态数据
/// 2. 将快照序列化为 JSON 返回
///
/// 可选参数：group_id, run_id（查询验证状态）, used_tokens, max_tokens（上下文压力计算）
///
/// @param state - 共享应用状态
/// @param args - 请求参数
fn handle_hud_snapshot(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    let snapshot = crate::hud::build_hud_snapshot(state, args);
    DaemonResponse::ok(serde_json::to_value(snapshot).unwrap_or_default())
}

// ============================================
// Group Handler 实现
// ============================================

/// handle_group_create: 创建新 Group
///
/// 在 groups_dir 下创建目录结构，写入 group.yaml 和 GroupCreate 事件
///
/// 必填参数：title
///
/// @param state - 共享应用状态
/// @param args - 请求参数
fn handle_group_create(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    let title = match args["title"].as_str() {
        Some(s) if !s.is_empty() => s,
        _ => return DaemonResponse::err("INVALID_ARGS", "缺少 title 字段"),
    };

    match crate::group::create_group(&state.groups_dir, title) {
        Ok(group) => DaemonResponse::ok(serde_json::to_value(group).unwrap_or_default()),
        Err(e) => DaemonResponse::err("GROUP_ERROR", e.to_string()),
    }
}

/// handle_group_show: 查看指定 Group 信息
///
/// 从 groups_dir 读取 group.yaml 并返回 GroupInfo
///
/// 必填参数：group_id
///
/// @param state - 共享应用状态
/// @param args - 请求参数
fn handle_group_show(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    let group_id = match args["group_id"].as_str() {
        Some(s) if !s.is_empty() => s,
        _ => return DaemonResponse::err("INVALID_ARGS", "缺少 group_id 字段"),
    };
    if let Err(resp) = validate_id(group_id, "group_id") { return resp; }

    let group_dir = state.groups_dir.join(group_id);
    match crate::group::load_group(&group_dir) {
        Ok(group) => DaemonResponse::ok(serde_json::to_value(group).unwrap_or_default()),
        Err(crate::group::GroupError::NotFound(_)) => DaemonResponse::err(
            "NOT_FOUND",
            format!("Group '{}' 不存在", group_id),
        ),
        Err(e) => DaemonResponse::err("GROUP_ERROR", e.to_string()),
    }
}

/// handle_group_delete: 删除指定 Group（清理整个目录）
///
/// 必填参数：group_id
///
/// @param state - 共享应用状态
/// @param args - 请求参数
fn handle_group_delete(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    let group_id = match args["group_id"].as_str() {
        Some(s) if !s.is_empty() => s,
        _ => return DaemonResponse::err("INVALID_ARGS", "缺少 group_id 字段"),
    };
    if let Err(resp) = validate_id(group_id, "group_id") { return resp; }

    match crate::group::delete_group(&state.groups_dir, group_id) {
        Ok(()) => DaemonResponse::ok(serde_json::json!({ "deleted": true, "group_id": group_id })),
        Err(crate::group::GroupError::NotFound(_)) => DaemonResponse::err(
            "NOT_FOUND",
            format!("Group '{}' 不存在", group_id),
        ),
        Err(e) => DaemonResponse::err("GROUP_ERROR", e.to_string()),
    }
}

/// handle_group_set_state: 设置 Group 状态
///
/// 必填参数：group_id, state（idle/running/paused）
///
/// @param state - 共享应用状态
/// @param args - 请求参数
fn handle_group_set_state(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    let group_id = match args["group_id"].as_str() {
        Some(s) if !s.is_empty() => s,
        _ => return DaemonResponse::err("INVALID_ARGS", "缺少 group_id 字段"),
    };
    if let Err(resp) = validate_id(group_id, "group_id") { return resp; }

    let state_str = match args["state"].as_str() {
        Some(s) => s,
        None => return DaemonResponse::err("INVALID_ARGS", "缺少 state 字段"),
    };

    let new_state = parse_group_state(state_str);
    let new_state = match new_state {
        Some(s) => s,
        None => return DaemonResponse::err(
            "INVALID_ARGS",
            format!("无效的 state '{}', 允许值: active, idle, paused, stopped", state_str),
        ),
    };

    let group_dir = state.groups_dir.join(group_id);
    let mut group = match crate::group::load_group(&group_dir) {
        Ok(g) => g,
        Err(crate::group::GroupError::NotFound(_)) => return DaemonResponse::err(
            "NOT_FOUND",
            format!("Group '{}' 不存在", group_id),
        ),
        Err(e) => return DaemonResponse::err("GROUP_ERROR", e.to_string()),
    };

    match crate::group::set_group_state(&state.groups_dir, &mut group, new_state) {
        Ok(()) => DaemonResponse::ok(serde_json::to_value(&group).unwrap_or_default()),
        Err(e) => DaemonResponse::err("GROUP_ERROR", e.to_string()),
    }
}

/// handle_group_set_state_shortcut: group_start/group_stop 快捷路由
///
/// group_start -> state = active
/// group_stop  -> state = idle
///
/// 必填参数：group_id
///
/// @param state - 共享应用状态
/// @param args - 请求参数
/// @param target_state - 目标状态字符串（"active" 或 "idle"）
fn handle_group_set_state_shortcut(
    state: &AppState,
    args: &serde_json::Value,
    target_state: &str,
) -> DaemonResponse {
    // 构造包含 state 字段的 args，复用 handle_group_set_state 逻辑
    let mut new_args = args.clone();
    if let Some(obj) = new_args.as_object_mut() {
        obj.insert("state".to_string(), serde_json::Value::String(target_state.to_string()));
    }
    handle_group_set_state(state, &new_args)
}

/// handle_groups_list: 列出所有 Group
///
/// 扫描 groups_dir，返回所有合法 Group 列表
///
/// 无必填参数
///
/// @param state - 共享应用状态
fn handle_groups_list(state: &AppState) -> DaemonResponse {
    match crate::group::list_groups(&state.groups_dir) {
        Ok(groups) => DaemonResponse::ok(serde_json::json!({
            "groups": serde_json::to_value(groups).unwrap_or_default(),
        })),
        Err(e) => DaemonResponse::err("GROUP_ERROR", e.to_string()),
    }
}

/// 解析 GroupState 字符串
///
/// 允许值：active, idle, paused, stopped
/// group_start 映射为 active，group_stop 映射为 idle
fn parse_group_state(s: &str) -> Option<ghostcode_types::group::GroupState> {
    match s {
        "active" | "running" => Some(ghostcode_types::group::GroupState::Active),
        "idle" => Some(ghostcode_types::group::GroupState::Idle),
        "paused" => Some(ghostcode_types::group::GroupState::Paused),
        "stopped" => Some(ghostcode_types::group::GroupState::Stopped),
        _ => None,
    }
}

// ============================================
// Actor Handler 实现
// ============================================

/// handle_actor_add: 向指定 Group 添加 Actor
///
/// 写入 ActorAdd 事件，更新 group.yaml
///
/// 必填参数：group_id, actor_id, display_name, role, runtime
///
/// @param state - 共享应用状态
/// @param args - 请求参数
fn handle_actor_add(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    let group_id = match args["group_id"].as_str() {
        Some(s) if !s.is_empty() => s,
        _ => return DaemonResponse::err("INVALID_ARGS", "缺少 group_id 字段"),
    };
    if let Err(resp) = validate_id(group_id, "group_id") { return resp; }

    let actor_id = match args["actor_id"].as_str() {
        Some(s) if !s.is_empty() => s,
        _ => return DaemonResponse::err("INVALID_ARGS", "缺少 actor_id 字段"),
    };
    if let Err(resp) = validate_id(actor_id, "actor_id") { return resp; }

    let display_name = args["display_name"]
        .as_str()
        .unwrap_or(actor_id)
        .to_string();

    // 解析 role（默认 peer）
    // ActorRole 枚举只有 Foreman 和 Peer 两种
    let role_str = args["role"].as_str().unwrap_or("peer");
    let role = parse_actor_role(role_str);
    let role = match role {
        Some(r) => r,
        None => return DaemonResponse::err(
            "INVALID_ARGS",
            format!("无效的 role '{}', 允许值: foreman, peer", role_str),
        ),
    };

    // 解析 runtime（默认 claude）
    let runtime_str = args["runtime"].as_str().unwrap_or("claude");
    let runtime = parse_runtime_kind(runtime_str);

    let group_dir = state.groups_dir.join(group_id);
    let mut group = match crate::group::load_group(&group_dir) {
        Ok(g) => g,
        Err(crate::group::GroupError::NotFound(_)) => return DaemonResponse::err(
            "NOT_FOUND",
            format!("Group '{}' 不存在", group_id),
        ),
        Err(e) => return DaemonResponse::err("GROUP_ERROR", e.to_string()),
    };

    let actor = ghostcode_types::actor::ActorInfo {
        actor_id: actor_id.to_string(),
        display_name,
        role,
        runtime,
        // 新注册的 Actor 默认处于未运行状态
        running: false,
        pid: None,
    };

    match crate::actor_mgmt::add_actor(&state.groups_dir, &mut group, actor) {
        Ok(()) => DaemonResponse::ok(serde_json::json!({
            "added": true,
            "actor_id": actor_id,
            "group_id": group_id,
        })),
        Err(crate::actor_mgmt::ActorError::DuplicateId(id)) => DaemonResponse::err(
            "DUPLICATE_ID",
            format!("Actor ID '{}' 已存在", id),
        ),
        Err(crate::actor_mgmt::ActorError::DuplicateForeman) => DaemonResponse::err(
            "DUPLICATE_FOREMAN",
            "该 Group 已存在 Foreman，不允许添加第二个",
        ),
        Err(e) => DaemonResponse::err("ACTOR_ERROR", e.to_string()),
    }
}

/// handle_actor_list: 列出指定 Group 中的所有 Actor
///
/// 从 group.yaml 读取 Actor 列表
///
/// 必填参数：group_id
///
/// @param state - 共享应用状态
/// @param args - 请求参数
fn handle_actor_list(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    let group_id = match args["group_id"].as_str() {
        Some(s) if !s.is_empty() => s,
        _ => return DaemonResponse::err("INVALID_ARGS", "缺少 group_id 字段"),
    };
    if let Err(resp) = validate_id(group_id, "group_id") { return resp; }

    let group_dir = state.groups_dir.join(group_id);
    let group = match crate::group::load_group(&group_dir) {
        Ok(g) => g,
        Err(crate::group::GroupError::NotFound(_)) => return DaemonResponse::err(
            "NOT_FOUND",
            format!("Group '{}' 不存在", group_id),
        ),
        Err(e) => return DaemonResponse::err("GROUP_ERROR", e.to_string()),
    };

    let actors = crate::actor_mgmt::list_actors(&group);
    DaemonResponse::ok(serde_json::json!({
        "actors": serde_json::to_value(actors).unwrap_or_default(),
        "count": actors.len(),
        "group_id": group_id,
    }))
}

/// handle_actor_remove: 从 Group 移除指定 Actor
///
/// 写入 ActorRemove 事件，更新 group.yaml
///
/// 必填参数：group_id, actor_id
///
/// @param state - 共享应用状态
/// @param args - 请求参数
fn handle_actor_remove(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    let group_id = match args["group_id"].as_str() {
        Some(s) if !s.is_empty() => s,
        _ => return DaemonResponse::err("INVALID_ARGS", "缺少 group_id 字段"),
    };
    if let Err(resp) = validate_id(group_id, "group_id") { return resp; }

    let actor_id = match args["actor_id"].as_str() {
        Some(s) if !s.is_empty() => s,
        _ => return DaemonResponse::err("INVALID_ARGS", "缺少 actor_id 字段"),
    };
    if let Err(resp) = validate_id(actor_id, "actor_id") { return resp; }

    let group_dir = state.groups_dir.join(group_id);
    let mut group = match crate::group::load_group(&group_dir) {
        Ok(g) => g,
        Err(crate::group::GroupError::NotFound(_)) => return DaemonResponse::err(
            "NOT_FOUND",
            format!("Group '{}' 不存在", group_id),
        ),
        Err(e) => return DaemonResponse::err("GROUP_ERROR", e.to_string()),
    };

    match crate::actor_mgmt::remove_actor(&state.groups_dir, &mut group, actor_id) {
        Ok(()) => DaemonResponse::ok(serde_json::json!({
            "removed": true,
            "actor_id": actor_id,
            "group_id": group_id,
        })),
        Err(crate::actor_mgmt::ActorError::NotFound(id)) => DaemonResponse::err(
            "NOT_FOUND",
            format!("Actor '{}' 不存在", id),
        ),
        Err(e) => DaemonResponse::err("ACTOR_ERROR", e.to_string()),
    }
}

/// 解析 ActorRole 字符串
///
/// ActorRole 只有 Foreman 和 Peer 两种，与 ghostcode-types/src/actor.rs 对齐
fn parse_actor_role(s: &str) -> Option<ghostcode_types::actor::ActorRole> {
    match s {
        "foreman" => Some(ghostcode_types::actor::ActorRole::Foreman),
        "peer" => Some(ghostcode_types::actor::ActorRole::Peer),
        _ => None,
    }
}

/// 解析 RuntimeKind 字符串
fn parse_runtime_kind(s: &str) -> ghostcode_types::actor::RuntimeKind {
    match s {
        "claude" => ghostcode_types::actor::RuntimeKind::Claude,
        "codex" => ghostcode_types::actor::RuntimeKind::Codex,
        "gemini" => ghostcode_types::actor::RuntimeKind::Gemini,
        other => ghostcode_types::actor::RuntimeKind::Custom(other.to_string()),
    }
}
// ============================================
