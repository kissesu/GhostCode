//! ghostcode_session_gate_close 工具实现
//!
//! 关闭 Session Gate，返回所有模型的合并输出
//! 若有模型尚未提交则返回 SESSION_INCOMPLETE 错误
//! 对应 Daemon op: "session_gate_close"
//!
//! @author Atlas.oi
//! @date 2026-03-07

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

/// 返回工具的 MCP inputSchema 定义
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_session_gate_close",
        "description": "Close the session gate. Returns merged output only if all required models have submitted (or bypassed). Returns SESSION_INCOMPLETE error if any model is missing.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "Session gate ID to close"
                }
            },
            "required": ["session_id"]
        }
    })
}

/// 执行 ghostcode_session_gate_close 工具
///
/// 业务逻辑说明：
/// 1. 提取必填参数 session_id，缺失则返回 MissingParam 错误
/// 2. 构造 DaemonRequest 发送 "session_gate_close" 操作
/// 3. Daemon 端校验所有 required models 是否已提交：
///    - 全部就绪：返回 partial/missing_models/submissions 合并结果
///    - 有缺失：返回 SESSION_INCOMPLETE 错误，透传给调用者
pub async fn execute(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    // 提取 session_id（必填参数）
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ToolError::MissingParam("session_id".to_string()))?;

    // 构造 Daemon 请求并调用
    let req = DaemonRequest::new(
        "session_gate_close",
        serde_json::json!({ "session_id": session_id }),
    );
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
