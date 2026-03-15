//! ghostcode_session_gate_abort 工具实现
//!
//! 强制终止 Session Gate，清理所有状态
//! 无论提交状态如何，均可终止（用户选择终止整个工作流时使用）
//! 对应 Daemon op: "session_gate_abort"
//!
//! @author Atlas.oi
//! @date 2026-03-07

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

/// 返回工具的 MCP inputSchema 定义
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_session_gate_abort",
        "description": "Abort a session gate regardless of submission status. Cleans up all state. Use when user chooses to terminate the workflow.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "Session gate ID to abort"
                },
                "reason": {
                    "type": "string",
                    "description": "Reason for aborting (for audit log)"
                }
            },
            "required": ["session_id"]
        }
    })
}

/// 执行 ghostcode_session_gate_abort 工具
///
/// 业务逻辑说明：
/// 1. 提取 session_id（必填，缺失返回 MissingParam）
/// 2. 提取 reason（可选，缺省为空字符串）
/// 3. 构造 DaemonRequest 发送 "session_gate_abort" 操作
/// 4. 调用 call_daemon，返回结果
pub async fn execute(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    // session_id 为必填参数
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ToolError::MissingParam("session_id".to_string()))?;

    // reason 为可选参数，缺省为空字符串
    let reason = args
        .get("reason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    let daemon_args = serde_json::json!({
        "session_id": session_id,
        "reason": reason
    });

    let req = DaemonRequest::new("session_gate_abort", daemon_args);
    let result = call_daemon(&ctx.daemon_addr, req).await?;
    Ok(result)
}

/// 注册表调用入口 -- 按值接收参数，供 ToolDescriptor::execute 使用
pub async fn execute_owned(
    args: serde_json::Value,
    ctx: ToolContext,
) -> Result<serde_json::Value, ToolError> {
    execute(&args, &ctx).await
}
