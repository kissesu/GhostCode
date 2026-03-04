//! ghostcode_actor_start 工具实现
//!
//! 启动指定 Actor（设置 enabled=true）
//! 对应 Daemon op: "actor_start"
//!
//! 参考: cccc/src/cccc/ports/mcp/handlers/cccc_group_actor.py:142-147
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_actor_start",
        "description": "Start an actor (set enabled=true). Only foreman can start actors.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "actor_id": {
                    "type": "string",
                    "description": "ID of the actor to start (required)"
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
        "actor_start",
        serde_json::json!({
            "group_id": ctx.group_id,
            "actor_id": actor_id,
            "by": ctx.actor_id
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
