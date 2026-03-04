//! ghostcode_skill_list 工具实现
//!
//! 列出当前 group 中可用的技能（skill）列表
//! 对应 Daemon op: "skill_list"
//!
//! @author Atlas.oi
//! @date 2026-03-04

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

/// 返回工具的 JSON Schema 定义
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_skill_list",
        "description": "List available skills in the current working group.",
        "inputSchema": {
            "type": "object",
            "properties": {},
            "required": []
        }
    })
}

/// 执行工具调用 — 通过 IPC 向 Daemon 查询当前 group 的技能列表
pub async fn execute(
    _args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    let req = DaemonRequest::new(
        "skill_list",
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
