//! ghostcode_group_list 工具实现
//!
//! 列出所有可用的工作组（group）信息
//! 对应 Daemon op: "groups"
//!
//! @author Atlas.oi
//! @date 2026-03-04

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

/// 返回工具的 JSON Schema 定义
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_group_list",
        "description": "List all available working groups.",
        "inputSchema": {
            "type": "object",
            "properties": {},
            "required": []
        }
    })
}

/// 执行工具调用 — 通过 IPC 向 Daemon 查询全部 group
pub async fn execute(
    _args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    // 注意：daemon 注册的 op 名称是 "groups"（非 "group_list"）
    // 保持与 dispatch.rs KNOWN_OPS 中的定义一致
    let req = DaemonRequest::new(
        "groups",
        serde_json::json!({}),
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
