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

use ghostcode_router::dag::{topological_sort, TaskNode};
use ghostcode_router::task_format::parse_task_format;
use ghostcode_types::ipc::{DaemonRequest, DaemonResponse};
use uuid::Uuid;

use crate::server::AppState;
use crate::verification::VerifyEvent;
use crate::{lifecycle, messaging::{send, inbox}, runner::HeadlessStatus};

/// Phase 1 + Phase 2 + Phase 3 所有已知的 op 字符串（32 个）
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
        "group_create" => stub(&req.op),
        "group_show" => stub(&req.op),
        "group_start" => stub(&req.op),
        "group_stop" => stub(&req.op),
        "group_delete" => stub(&req.op),
        "group_set_state" => stub(&req.op),
        "groups" => stub(&req.op),

        // === Actor 管理 ===
        "actor_add" => stub(&req.op),
        "actor_list" => stub(&req.op),
        "actor_start" => handle_actor_start(state, &req.args).await,
        "actor_stop" => handle_actor_stop(state, &req.args).await,
        "actor_remove" => stub(&req.op),

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

    match lifecycle::start_actor(state, group_id, actor_id).await {
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

/// 占位 handler（后续任务逐步实现）
fn stub(op: &str) -> DaemonResponse {
    DaemonResponse::err(
        "NOT_IMPLEMENTED",
        format!("operation '{}' is not yet implemented", op),
    )
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
    let backend = args["backend"].as_str().unwrap_or("claude");
    let workdir = args["workdir"].as_str();

    // 检查后端写入权限（代码主权守卫）
    let can_write = state.routing.sovereignty.can_write(backend);

    // 生成 UUID task_id
    let task_id = Uuid::new_v4().to_string();

    // 注册任务到路由状态表
    state.routing.register_task(group_id, &task_id, backend).await;

    DaemonResponse::ok(serde_json::json!({
        "task_id": task_id,
        "backend": backend,
        "can_write": can_write,
        "group_id": group_id,
        "task_text": task_text,
        "workdir": workdir,
    }))
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

    // 生成并行任务组的 task_id，并注册为 pending
    let task_id = Uuid::new_v4().to_string();
    state.routing.register_task(group_id, &task_id, "parallel").await;

    DaemonResponse::ok(serde_json::json!({
        "task_id": task_id,
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
/// 当前返回空列表框架，SessionStore 集成在后续迭代中完成。
///
/// 必填参数：group_id
///
/// @param _state - 共享应用状态（当前未使用）
/// @param args - 请求参数
async fn handle_session_list(_state: &AppState, args: &serde_json::Value) -> DaemonResponse {
    // 提取必填参数
    let group_id = match args["group_id"].as_str() {
        Some(v) => v,
        None => return DaemonResponse::err("INVALID_ARGS", "missing required field: group_id"),
    };

    if let Err(resp) = validate_id(group_id, "group_id") {
        return resp;
    }

    // SessionStore 集成在后续迭代完成，当前返回空框架
    // 参考: ghostcode-router/src/session.rs - SessionStore 实现
    DaemonResponse::ok(serde_json::json!({
        "sessions": [],
        "group_id": group_id,
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
