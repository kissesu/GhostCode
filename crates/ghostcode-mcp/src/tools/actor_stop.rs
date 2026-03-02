//! ghostcode_actor_stop 工具实现
//!
//! 停止指定 Actor（设置 enabled=false）
//! 对应 Daemon op: "actor_stop"
//!
//! 参考: cccc/src/cccc/ports/mcp/handlers/cccc_group_actor.py:150-155
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_actor_stop",
        "description": "Stop an actor (set enabled=false). Foreman can stop any actor; peer can only stop themselves.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "actor_id": {
                    "type": "string",
                    "description": "ID of the actor to stop (required)"
                }
            },
            "required": ["actor_id"]
        }
    })
}

pub async fn execute(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    let actor_id = args
        .get("actor_id")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ToolError::MissingParam("actor_id".to_string()))?;

    let req = DaemonRequest::new(
        "actor_stop",
        serde_json::json!({
            "group_id": ctx.group_id,
            "actor_id": actor_id,
            "by": ctx.actor_id
        }),
    );

    let result = call_daemon(&ctx.daemon_addr, req).await?;
    Ok(result)
}
