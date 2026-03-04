//! ghostcode_route_task 工具实现
//!
//! 提交任务到多模型路由引擎
//! 对应 Daemon op: "route_task"
//!
//! @author Atlas.oi
//! @date 2026-03-02

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_route_task",
        "description": "Submit a task to the multi-model routing engine",
        "inputSchema": {
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "Task text to execute (required)"
                },
                "backend": {
                    "type": "string",
                    "description": "Target backend: codex/claude/gemini (optional, default: claude)"
                },
                "workdir": {
                    "type": "string",
                    "description": "Working directory for the task (optional)"
                }
            },
            "required": ["task"]
        }
    })
}

pub async fn execute(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    // 提取 task（必填）
    let task_text = args
        .get("task")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ToolError::MissingParam("task".to_string()))?;

    // 提取 backend（可选，默认 "claude"）
    let backend = args
        .get("backend")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "claude".to_string());

    // 提取 workdir（可选）
    let workdir = args
        .get("workdir")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    // 构造 DaemonRequest
    let req = DaemonRequest::new(
        "route_task",
        serde_json::json!({
            "group_id": ctx.group_id,
            "task_text": task_text,
            "backend": backend,
            "workdir": workdir
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
