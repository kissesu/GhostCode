//! ghostcode_verification_status 工具实现
//!
//! 获取当前工作组的验证状态（SovereigntyGuard 检查结果）
//! 对应 Daemon op: "verification_status"
//!
//! @author Atlas.oi
//! @date 2026-03-04

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

/// 返回工具的 JSON Schema 定义
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_verification_status",
        "description": "Get the verification status of the current working group, including SovereigntyGuard check results.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "run_id": {
                    "type": "string",
                    "description": "Verification run ID to query status for"
                }
            },
            "required": ["run_id"]
        }
    })
}

/// 执行工具调用 — 通过 IPC 向 Daemon 获取验证状态
///
/// daemon 要求参数：group_id + run_id（非 actor_id）
/// run_id 从工具调用参数中获取，group_id 从上下文中获取
pub async fn execute(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    // daemon handle_verification_status 要求 group_id + run_id
    let run_id = args["run_id"].as_str().unwrap_or_default();
    let req = DaemonRequest::new(
        "verification_status",
        serde_json::json!({
            "group_id": ctx.group_id,
            "run_id": run_id
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
