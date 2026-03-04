//! ghostcode_group_info 工具实现
//!
//! 获取当前工作组的基本信息
//! 对应 Daemon op: "group_show"
//!
//! 参考: cccc/src/cccc/ports/mcp/handlers/cccc_group_actor.py:63-67
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_group_info",
        "description": "Get information about the current working group.",
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
        "group_show",
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
