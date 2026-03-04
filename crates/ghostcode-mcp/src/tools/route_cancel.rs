//! ghostcode_route_cancel 工具实现
//!
//! 取消正在执行的路由任务
//! 对应 Daemon op: "route_cancel"
//!
//! @author Atlas.oi
//! @date 2026-03-02

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_route_cancel",
        "description": "Cancel a running routed task",
        "inputSchema": {
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "ID of the task to cancel (required)"
                }
            },
            "required": ["task_id"]
        }
    })
}

pub async fn execute(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    // 提取 task_id（必填）
    let task_id = args
        .get("task_id")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ToolError::MissingParam("task_id".to_string()))?;

    // 构造 DaemonRequest
    let req = DaemonRequest::new(
        "route_cancel",
        serde_json::json!({
            "group_id": ctx.group_id,
            "task_id": task_id
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
