//! GhostCode MCP 工具模块
//!
//! 定义所有 MCP 工具的公共类型（ToolContext / ToolError）和分发函数（dispatch_tool）
//! 每个工具实现在独立子模块中，遵循统一的 schema() + execute() 接口
//!
//! 参考: cccc/src/cccc/ports/mcp/handlers/ - 工具处理函数组织方式
//!
//! @author Atlas.oi
//! @date 2026-03-01

pub mod actor_list;
pub mod actor_start;
pub mod actor_stop;
pub mod group_info;
pub mod inbox_list;
pub mod inbox_mark_all_read;
pub mod inbox_mark_read;
pub mod message_send;
pub mod route_cancel;
pub mod route_status;
pub mod route_task;

use anyhow::Result;

// ============================================================
// 工具上下文
// 由 server.rs 构造，注入到每个工具的 execute() 调用中
// ============================================================

/// MCP 工具调用上下文
///
/// 封装工具执行所需的环境信息，避免每次从环境变量重复读取
/// group_id 和 actor_id 来自环境变量，不从 MCP 参数读取
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Daemon Unix Socket 路径（由主程序启动参数提供）
    pub daemon_addr: std::path::PathBuf,
    /// GhostCode 工作组 ID（从 GHOSTCODE_GROUP_ID 读取）
    pub group_id: String,
    /// Actor ID（从 GHOSTCODE_ACTOR_ID 读取）
    pub actor_id: String,
}

impl ToolContext {
    /// 从环境变量构造 ToolContext
    ///
    /// 若环境变量缺失则使用空字符串（Daemon 会返回相应错误）
    pub fn from_env(daemon_addr: impl Into<std::path::PathBuf>) -> Self {
        Self {
            daemon_addr: daemon_addr.into(),
            group_id: std::env::var("GHOSTCODE_GROUP_ID").unwrap_or_default(),
            actor_id: std::env::var("GHOSTCODE_ACTOR_ID").unwrap_or_default(),
        }
    }
}

// ============================================================
// 工具错误类型
// ============================================================

/// MCP 工具调用错误
///
/// 区分参数验证错误和 Daemon 调用错误
/// 上层（server.rs）根据错误类型构造合适的 MCP 错误响应
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// 缺少必填参数
    /// 参考: cccc/src/cccc/ports/mcp/handlers/cccc_core.py:208-210 - 空参数检查模式
    #[error("missing required parameter: {0}")]
    MissingParam(String),

    /// 参数类型或值无效
    #[error("invalid parameter '{name}': {reason}")]
    InvalidParam {
        /// 参数名
        name: String,
        /// 无效原因描述
        reason: String,
    },

    /// Daemon 调用失败（连接失败、协议错误、业务错误）
    /// 禁止将此错误降级为空结果或默认值
    #[error("daemon error: {0}")]
    Daemon(#[from] anyhow::Error),
}

impl ToolError {
    /// 转换为 MCP isError 响应内容（JSON 字符串）
    pub fn to_mcp_error_json(&self) -> String {
        let (code, message) = match self {
            ToolError::MissingParam(name) => (
                "missing_param".to_string(),
                format!("missing required parameter: {}", name),
            ),
            ToolError::InvalidParam { name, reason } => (
                "invalid_param".to_string(),
                format!("invalid parameter '{}': {}", name, reason),
            ),
            ToolError::Daemon(e) => ("daemon_error".to_string(), e.to_string()),
        };
        serde_json::json!({
            "error": {
                "code": code,
                "message": message
            }
        })
        .to_string()
    }
}

// ============================================================
// 工具 Schema 列表（用于 tools/list 响应）
// ============================================================

/// 返回所有工具的 schema 定义列表
///
/// 由 server.rs 的 tools/list 处理器调用
/// 参考: cccc/src/cccc/ports/mcp/toolspecs.py - MCP_TOOLS 结构
pub fn all_tool_schemas() -> Vec<serde_json::Value> {
    vec![
        message_send::schema(),
        inbox_list::schema(),
        inbox_mark_read::schema(),
        inbox_mark_all_read::schema(),
        actor_list::schema(),
        actor_start::schema(),
        actor_stop::schema(),
        group_info::schema(),
        route_task::schema(),
        route_status::schema(),
        route_cancel::schema(),
    ]
}

// ============================================================
// 工具分发函数
// 由 server.rs 的 tools/call 处理器调用
// ============================================================

/// 根据工具名分发工具调用
///
/// 业务逻辑：
/// 1. 匹配 tool_name 到对应工具模块
/// 2. 调用该模块的 execute(args, ctx)
/// 3. 返回工具结果或错误
pub async fn dispatch_tool(
    tool_name: &str,
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    match tool_name {
        "ghostcode_message_send"       => message_send::execute(args, ctx).await,
        "ghostcode_inbox_list"         => inbox_list::execute(args, ctx).await,
        "ghostcode_inbox_mark_read"    => inbox_mark_read::execute(args, ctx).await,
        "ghostcode_inbox_mark_all_read" => inbox_mark_all_read::execute(args, ctx).await,
        "ghostcode_actor_list"         => actor_list::execute(args, ctx).await,
        "ghostcode_actor_start"        => actor_start::execute(args, ctx).await,
        "ghostcode_actor_stop"         => actor_stop::execute(args, ctx).await,
        "ghostcode_group_info"         => group_info::execute(args, ctx).await,
        "ghostcode_route_task"         => route_task::execute(args, ctx).await,
        "ghostcode_route_status"       => route_status::execute(args, ctx).await,
        "ghostcode_route_cancel"       => route_cancel::execute(args, ctx).await,
        _ => Err(ToolError::InvalidParam {
            name: "tool_name".to_string(),
            reason: format!("unknown tool: {}", tool_name),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --------------------------------------------------------
    // 测试辅助：构造无 Daemon 的 ToolContext（指向不存在的 Socket）
    // --------------------------------------------------------
    fn mock_ctx() -> ToolContext {
        ToolContext {
            daemon_addr: std::path::PathBuf::from("/nonexistent/ghostcode.sock"),
            group_id: "test-group".to_string(),
            actor_id: "test-actor".to_string(),
        }
    }

    // --------------------------------------------------------
    // 测试 1: all_tool_schemas 返回恰好 8 个工具定义
    // --------------------------------------------------------
    #[test]
    fn all_schemas_returns_11_tools() {
        let schemas = all_tool_schemas();
        assert_eq!(schemas.len(), 11, "必须恰好返回 11 个工具定义");

        // 验证每个 schema 都包含 name / description / inputSchema 字段
        for schema in &schemas {
            assert!(schema.get("name").is_some(), "工具 schema 必须包含 name");
            assert!(schema.get("description").is_some(), "工具 schema 必须包含 description");
            assert!(schema.get("inputSchema").is_some(), "工具 schema 必须包含 inputSchema");
        }
    }

    // --------------------------------------------------------
    // 测试 2: dispatch_tool 对未知工具返回 ToolError::InvalidParam
    // --------------------------------------------------------
    #[tokio::test]
    async fn dispatch_unknown_tool_returns_invalid_param() {
        let ctx = mock_ctx();
        let result = dispatch_tool("unknown_tool", &serde_json::json!({}), &ctx).await;
        assert!(matches!(result, Err(ToolError::InvalidParam { .. })), "未知工具必须返回 InvalidParam 错误");
    }

    // --------------------------------------------------------
    // 测试 3: message_send 缺少 text 返回 MissingParam
    // --------------------------------------------------------
    #[tokio::test]
    async fn message_send_missing_text_returns_error() {
        let ctx = mock_ctx();
        let result = super::message_send::execute(&serde_json::json!({}), &ctx).await;
        assert!(
            matches!(result, Err(ToolError::MissingParam(ref name)) if name == "text"),
            "缺少 text 时必须返回 MissingParam(\"text\")"
        );
    }

    // --------------------------------------------------------
    // 测试 4: inbox_mark_read 缺少 event_id 返回 MissingParam
    // --------------------------------------------------------
    #[tokio::test]
    async fn inbox_mark_read_missing_event_id_returns_error() {
        let ctx = mock_ctx();
        let result = super::inbox_mark_read::execute(&serde_json::json!({}), &ctx).await;
        assert!(
            matches!(result, Err(ToolError::MissingParam(ref name)) if name == "event_id"),
            "缺少 event_id 时必须返回 MissingParam(\"event_id\")"
        );
    }

    // --------------------------------------------------------
    // 测试 5: inbox_mark_read 空字符串 event_id 返回 MissingParam
    // --------------------------------------------------------
    #[tokio::test]
    async fn inbox_mark_read_empty_event_id_returns_error() {
        let ctx = mock_ctx();
        let result = super::inbox_mark_read::execute(
            &serde_json::json!({"event_id": ""}),
            &ctx,
        ).await;
        assert!(
            matches!(result, Err(ToolError::MissingParam(ref name)) if name == "event_id"),
            "空字符串 event_id 必须视为缺失"
        );
    }

    // --------------------------------------------------------
    // 测试 6: actor_start 缺少 actor_id 返回 MissingParam
    // --------------------------------------------------------
    #[tokio::test]
    async fn actor_start_missing_actor_id_returns_error() {
        let ctx = mock_ctx();
        let result = super::actor_start::execute(&serde_json::json!({}), &ctx).await;
        assert!(
            matches!(result, Err(ToolError::MissingParam(ref name)) if name == "actor_id"),
            "缺少 actor_id 时必须返回 MissingParam(\"actor_id\")"
        );
    }

    // --------------------------------------------------------
    // 测试 7: actor_stop 缺少 actor_id 返回 MissingParam
    // --------------------------------------------------------
    #[tokio::test]
    async fn actor_stop_missing_actor_id_returns_error() {
        let ctx = mock_ctx();
        let result = super::actor_stop::execute(&serde_json::json!({}), &ctx).await;
        assert!(
            matches!(result, Err(ToolError::MissingParam(ref name)) if name == "actor_id"),
            "缺少 actor_id 时必须返回 MissingParam(\"actor_id\")"
        );
    }

    // --------------------------------------------------------
    // 测试 8: ToolError::to_mcp_error_json 格式正确
    // --------------------------------------------------------
    #[test]
    fn tool_error_missing_param_json_format() {
        let err = ToolError::MissingParam("text".to_string());
        let json_str = err.to_mcp_error_json();
        let v: serde_json::Value = serde_json::from_str(&json_str)
            .expect("to_mcp_error_json 必须返回合法 JSON");
        assert_eq!(v["error"]["code"], "missing_param", "错误码必须为 missing_param");
        assert!(v["error"]["message"].as_str().is_some(), "错误信息必须为字符串");
    }

    // --------------------------------------------------------
    // 测试 9: message_send 参数解析通过后到达 Daemon 阶段
    // --------------------------------------------------------
    #[tokio::test]
    async fn message_send_with_text_reaches_daemon_stage() {
        let ctx = mock_ctx();
        let result = super::message_send::execute(
            &serde_json::json!({"text": "hello", "to": "peer-1"}),
            &ctx,
        ).await;
        assert!(
            matches!(result, Err(ToolError::Daemon(_))),
            "参数解析通过后应到达 Daemon 调用阶段（返回连接错误而非参数错误）"
        );
    }

    // --------------------------------------------------------
    // 测试 10: 验证工具名称列表与 dispatch_tool 路由一致
    // --------------------------------------------------------
    #[test]
    fn all_schema_names_match_dispatch_routes() {
        let schemas = all_tool_schemas();
        let expected_names = [
            "ghostcode_message_send",
            "ghostcode_inbox_list",
            "ghostcode_inbox_mark_read",
            "ghostcode_inbox_mark_all_read",
            "ghostcode_actor_list",
            "ghostcode_actor_start",
            "ghostcode_actor_stop",
            "ghostcode_group_info",
            "ghostcode_route_task",
            "ghostcode_route_status",
            "ghostcode_route_cancel",
        ];

        let actual_names: Vec<&str> = schemas
            .iter()
            .filter_map(|s| s.get("name").and_then(|n| n.as_str()))
            .collect();

        for expected in &expected_names {
            assert!(
                actual_names.contains(expected),
                "工具 '{}' 必须出现在 all_tool_schemas() 中",
                expected
            );
        }
    }
}
