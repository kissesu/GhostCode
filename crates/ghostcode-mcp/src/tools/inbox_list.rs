//! ghostcode_inbox_list 工具实现
//!
//! 列出当前 Actor 的未读消息
//! 对应 Daemon op: "inbox_list"
//!
//! 参考: cccc/src/cccc/ports/mcp/handlers/cccc_core.py:201-203
//! 参考: cccc/src/cccc/ports/mcp/toolspecs.py:80-94
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_inbox_list",
        "description": "List unread inbox messages for the current actor.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "default": 50,
                    "minimum": 1,
                    "maximum": 1000,
                    "description": "Maximum number of messages to return"
                }
            },
            "required": []
        }
    })
}

pub async fn execute(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|n| n.clamp(1, 1000))
        .unwrap_or(50);

    let req = DaemonRequest::new(
        "inbox_list",
        serde_json::json!({
            "group_id": ctx.group_id,
            "actor_id": ctx.actor_id,
            "by": ctx.actor_id,
            "limit": limit,
            "kind_filter": "all"
        }),
    );

    let result = call_daemon(&ctx.daemon_addr, req).await?;
    Ok(result)
}
