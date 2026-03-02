//! ghostcode_inbox_mark_all_read 工具实现
//!
//! 将当前 Actor 的所有未读消息标记为已读
//! 对应 Daemon op: "inbox_mark_all_read"
//!
//! 参考: cccc/src/cccc/ports/mcp/handlers/cccc_core.py:216-219
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_inbox_mark_all_read",
        "description": "Mark all unread messages as read for the current actor.",
        "inputSchema": {
            "type": "object",
            "properties": {},
            "required": []
        }
    })
}

pub async fn execute(
    _args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    let req = DaemonRequest::new(
        "inbox_mark_all_read",
        serde_json::json!({
            "group_id": ctx.group_id,
            "actor_id": ctx.actor_id,
            "kind_filter": "all",
            "by": ctx.actor_id
        }),
    );

    let result = call_daemon(&ctx.daemon_addr, req).await?;
    Ok(result)
}
