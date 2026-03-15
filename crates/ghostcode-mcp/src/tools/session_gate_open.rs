//! ghostcode_session_gate_open 工具实现
//!
//! 开启新的 Session Gate，强制要求指定模型提交输出后才能关闭
//! 对应 Daemon op: "session_gate_open"
//!
//! @author Atlas.oi
//! @date 2026-03-07

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

/// 返回工具的 MCP inputSchema 定义
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_session_gate_open",
        "description": "Open a new session gate requiring specified models to submit their outputs before closing.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "command_type": {
                    "type": "string",
                    "enum": ["research", "plan", "execute", "review"],
                    "description": "The gc:* command type initiating the session gate"
                },
                "required_models": {
                    "type": "array",
                    "items": { "type": "string", "enum": ["codex", "gemini"] },
                    "description": "Models required to submit before the gate can be closed"
                }
            },
            "required": ["command_type", "required_models"]
        }
    })
}

/// 执行 ghostcode_session_gate_open 工具
///
/// 业务逻辑说明：
/// 1. 提取并验证 command_type 参数（必填）
/// 2. 提取并验证 required_models 数组（必填）
/// 3. 构造 DaemonRequest 并调用 Daemon，返回包含 session_id 的结果
pub async fn execute(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    // ============================================
    // 第一步：提取 command_type 参数
    // 必填字段，缺失则返回 MissingParam 错误
    // ============================================
    let command_type = args
        .get("command_type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ToolError::MissingParam("command_type".to_string()))?;

    // ============================================
    // 第二步：提取 required_models 数组
    // 必填字段，缺失或非数组则返回 MissingParam 错误
    // ============================================
    let required_models: Vec<String> = args
        .get("required_models")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .ok_or_else(|| ToolError::MissingParam("required_models".to_string()))?;

    // ============================================
    // 第三步：构造 DaemonRequest 并调用 Daemon
    // 发送 session_gate_open 操作，返回包含 session_id 的响应
    // ============================================
    let daemon_args = serde_json::json!({
        "command_type": command_type,
        "required_models": required_models
    });

    let req = DaemonRequest::new("session_gate_open", daemon_args);
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
