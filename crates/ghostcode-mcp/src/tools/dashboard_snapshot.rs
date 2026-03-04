//! ghostcode_dashboard_snapshot 工具实现
//!
//! 获取当前工作组的 Dashboard 快照数据，包含 actor 状态、消息统计等聚合信息
//! 对应 Daemon op: "dashboard_snapshot"
//!
//! @author Atlas.oi
//! @date 2026-03-04

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

/// 返回工具的 JSON Schema 定义
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_dashboard_snapshot",
        "description": "Get a dashboard snapshot of the current working group, including actor statuses and message statistics.",
        "inputSchema": {
            "type": "object",
            "properties": {},
            "required": []
        }
    })
}

/// 执行工具调用 — 通过 IPC 向 Daemon 获取 dashboard 快照
pub async fn execute(
    _args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    let req = DaemonRequest::new(
        "dashboard_snapshot",
        serde_json::json!({
            "group_id": ctx.group_id
        }),
    );

    let result = call_daemon(&ctx.daemon_addr, req).await?;
    Ok(result)
}

/// 注册表调用入口 — 按值接收参数，供 ToolDescriptor::execute 使用
pub async fn execute_owned(
    args: serde_json::Value,
    ctx: ToolContext,
) -> Result<serde_json::Value, ToolError> {
    execute(&args, &ctx).await
}
