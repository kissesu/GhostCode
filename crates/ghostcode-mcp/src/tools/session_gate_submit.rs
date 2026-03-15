//! ghostcode_session_gate_submit 工具实现
//!
//! 向已开启的 Session Gate 提交模型输出
//! 支持 bypass 模式（额度用完、网络故障等情况的出口）
//! 对应 Daemon op: "session_gate_submit"
//!
//! @author Atlas.oi
//! @date 2026-03-07

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

/// 返回工具的 MCP inputSchema 定义
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_session_gate_submit",
        "description": "Submit a model's analysis output to an open session gate.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "Session gate ID returned by ghostcode_session_gate_open"
                },
                "model": {
                    "type": "string",
                    "enum": ["codex", "gemini"],
                    "description": "The model submitting its output"
                },
                "output_type": {
                    "type": "string",
                    "description": "Type of output: 'findings', 'analysis', 'prototype_ref', etc."
                },
                "data": {
                    "type": "object",
                    "description": "The model's output data (JSON object)"
                },
                "bypass": {
                    "type": "boolean",
                    "default": false,
                    "description": "Set to true when model is unavailable (quota exceeded, network error)"
                },
                "bypass_reason": {
                    "type": "string",
                    "description": "Reason for bypass (required when bypass=true)"
                }
            },
            "required": ["session_id", "model", "output_type", "data"]
        }
    })
}

/// 执行 ghostcode_session_gate_submit 工具
///
/// 业务逻辑说明：
/// 1. 提取并验证必填参数 session_id, model, output_type, data
/// 2. 提取可选参数 bypass, bypass_reason
/// 3. 构造 DaemonRequest 发送至 Daemon 处理
pub async fn execute(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    // 提取 session_id（必填）
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ToolError::MissingParam("session_id".to_string()))?;

    // 提取 model（必填），限定枚举值 codex / gemini
    let model = args
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ToolError::MissingParam("model".to_string()))?;

    // 提取 output_type（必填）
    let output_type = args
        .get("output_type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ToolError::MissingParam("output_type".to_string()))?;

    // 提取 data（必填，若缺失则使用空对象兜底）
    let data = args
        .get("data")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    // 提取 bypass（可选，默认 false）
    let bypass = args
        .get("bypass")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // 提取 bypass_reason（可选）
    let bypass_reason = args
        .get("bypass_reason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    // 构造 Daemon 请求参数
    let daemon_args = serde_json::json!({
        "session_id": session_id,
        "model": model,
        "output_type": output_type,
        "data": data,
        "bypass": bypass,
        "bypass_reason": bypass_reason
    });

    let req = DaemonRequest::new("session_gate_submit", daemon_args);
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
