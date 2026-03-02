//! ghostcode_inbox_mark_read 工具实现
//!
//! 将指定事件 ID 的消息标记为已读
//! 对应 Daemon op: "inbox_mark_read"
//!
//! 参考: cccc/src/cccc/ports/mcp/handlers/cccc_core.py:207-213
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_inbox_mark_read",
        "description": "Mark a specific message as read by its event ID.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "event_id": {
                    "type": "string",
                    "description": "Event ID of the message to mark as read (required)"
                }
            },
            "required": ["event_id"]
        }
    })
}

pub async fn execute(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    let event_id = args
        .get("event_id")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ToolError::MissingParam("event_id".to_string()))?;

    let req = DaemonRequest::new(
        "inbox_mark_read",
        serde_json::json!({
            "group_id": ctx.group_id,
            "actor_id": ctx.actor_id,
            "event_id": event_id,
            "by": ctx.actor_id
        }),
    );

    let result = call_daemon(&ctx.daemon_addr, req).await?;
    Ok(result)
}
