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
                },
                "display_name": {
                    "type": "string",
                    "description": "Human-readable display name for the actor (optional)"
                },
                "agent_type": {
                    "type": "string",
                    "description": "Agent type identifier, e.g. 'feature-dev:code-reviewer' (optional)"
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

    // 提取可选的 display_name 和 agent_type 参数
    let display_name = args.get("display_name").and_then(|v| v.as_str());
    let agent_type = args.get("agent_type").and_then(|v| v.as_str());

    let mut req_args = serde_json::json!({
        "group_id": ctx.group_id,
        "actor_id": actor_id,
        "by": ctx.actor_id
    });
    // 仅在有值时附加可选字段，避免发送 null
    if let Some(dn) = display_name {
        req_args["display_name"] = serde_json::Value::String(dn.to_string());
    }
    if let Some(at) = agent_type {
        req_args["agent_type"] = serde_json::Value::String(at.to_string());
    }

    let req = DaemonRequest::new("actor_start", req_args);

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
