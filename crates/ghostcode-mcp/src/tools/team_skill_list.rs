//! ghostcode_team_skill_list 工具实现
//!
//! 跨 group 聚合技能列表，返回整个团队（所有 group）的技能汇总
//! 对应 Daemon op: "team_skill_list"
//!
//! @author Atlas.oi
//! @date 2026-03-04

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

/// 返回工具的 JSON Schema 定义
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_team_skill_list",
        "description": "List skills aggregated across all groups in the team.",
        "inputSchema": {
            "type": "object",
            "properties": {},
            "required": []
        }
    })
}

/// 执行工具调用 — 通过 IPC 向 Daemon 获取跨 group 聚合技能列表
pub async fn execute(
    _args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    let req = DaemonRequest::new(
        "team_skill_list",
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
