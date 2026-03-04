//! ghostcode_message_send 工具实现
//!
//! 发送消息给工作组内的 Agent
//! 对应 Daemon op: "send"
//!
//! 参考: cccc/src/cccc/ports/mcp/handlers/cccc_messaging.py:15-61
//! 参考: cccc/src/cccc/ports/mcp/toolspecs.py:116-136
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

/// 返回工具的 MCP inputSchema 定义
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_message_send",
        "description": "Send a visible chat message to the group or specific actors.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Message content (required)"
                },
                "to": {
                    "description": "Target actor IDs. Empty = broadcast to all. String or array of strings.",
                    "anyOf": [
                        {"type": "string"},
                        {"type": "array", "items": {"type": "string"}}
                    ]
                },
                "reply_to": {
                    "type": "string",
                    "description": "Event ID to reply to (optional)"
                },
                "priority": {
                    "type": "string",
                    "enum": ["normal", "attention"],
                    "default": "normal"
                }
            },
            "required": ["text"]
        }
    })
}

/// 执行 ghostcode_message_send 工具（stub - 参数验证 + Daemon 调用）
pub async fn execute(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    let text = args
        .get("text")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ToolError::MissingParam("text".to_string()))?;

    let to: Vec<String> = match args.get("to") {
        Some(serde_json::Value::String(s)) => {
            if s.is_empty() { vec![] } else { vec![s.clone()] }
        }
        Some(serde_json::Value::Array(arr)) => {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .filter(|s| !s.is_empty())
                .collect()
        }
        _ => vec![],
    };

    let reply_to = args
        .get("reply_to")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    let priority = {
        let p = args
            .get("priority")
            .and_then(|v| v.as_str())
            .unwrap_or("normal");
        match p {
            "normal" | "attention" => p.to_string(),
            _ => "normal".to_string(),
        }
    };

    let (op, daemon_args) = if reply_to.is_empty() {
        (
            "send",
            serde_json::json!({
                "group_id": ctx.group_id,
                "text": text,
                "by": ctx.actor_id,
                "to": to,
                "path": "",
                "priority": priority,
                "reply_required": false
            }),
        )
    } else {
        (
            "reply",
            serde_json::json!({
                "group_id": ctx.group_id,
                "text": text,
                "by": ctx.actor_id,
                "reply_to": reply_to,
                "to": to,
                "priority": priority,
                "reply_required": false
            }),
        )
    };

    let req = DaemonRequest::new(op, daemon_args);
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
