//! GhostCode MCP 工具模块
//!
//! 定义所有 MCP 工具的公共类型（ToolContext / ToolError）和分发函数（dispatch_tool）
//! 每个工具实现在独立子模块中，遵循统一的 schema() + execute() 接口
//!
//! 注册表模式：通过 registry() 获取全部工具描述符，通过 find_tool() 按名查找
//! 取代原有的线性 match 分发，使新增工具只需修改注册表数组即可
//!
//! 参考: cccc/src/cccc/ports/mcp/handlers/ - 工具处理函数组织方式
//!
//! @author Atlas.oi
//! @date 2026-03-04

// ============================================================
// 原有 11 个工具模块
// ============================================================
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

// ============================================================
// 新增 5 个工具模块
// ============================================================
pub mod dashboard_snapshot;
pub mod group_list;
pub mod skill_list;
pub mod team_skill_list;
pub mod verification_status;

use anyhow::Result;
use std::future::Future;
use std::pin::Pin;

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
// 工具描述符 — 注册表核心类型
// ============================================================

/// 工具异步执行函数的返回类型别名
///
/// 避免在 ToolDescriptor 中出现过于复杂的内联类型
pub type ToolFuture = Pin<Box<dyn Future<Output = Result<serde_json::Value, ToolError>> + Send>>;

/// MCP 工具描述符
///
/// 每个工具通过实现此结构体注册到全局注册表
/// 注册表替代原有的线性 match，新增工具只需在 REGISTRY 数组中追加条目
pub struct ToolDescriptor {
    /// 工具唯一名称，与 MCP tools/call 的 name 字段对应
    pub name: &'static str,
    /// 工具描述（仅供注册表元信息使用，详细 schema 由 schema 函数提供）
    pub description: &'static str,
    /// 返回工具的 JSON Schema（用于 tools/list 响应）
    pub schema: fn() -> serde_json::Value,
    /// 异步执行工具调用，返回结果或错误
    pub execute: fn(args: serde_json::Value, ctx: ToolContext) -> ToolFuture,
}

// ============================================================
// 全局工具注册表
// 新增工具只需在此数组末尾追加 ToolDescriptor 条目
// ============================================================

/// 所有 MCP 工具的静态注册表
static REGISTRY: &[ToolDescriptor] = &[
    // --------------------------------------------------------
    // 原有 11 个工具
    // --------------------------------------------------------
    ToolDescriptor {
        name: "ghostcode_message_send",
        description: "Send a message to another actor.",
        schema: message_send::schema,
        execute: |args, ctx| Box::pin(message_send::execute_owned(args, ctx)),
    },
    ToolDescriptor {
        name: "ghostcode_inbox_list",
        description: "List messages in the inbox.",
        schema: inbox_list::schema,
        execute: |args, ctx| Box::pin(inbox_list::execute_owned(args, ctx)),
    },
    ToolDescriptor {
        name: "ghostcode_inbox_mark_read",
        description: "Mark a message as read.",
        schema: inbox_mark_read::schema,
        execute: |args, ctx| Box::pin(inbox_mark_read::execute_owned(args, ctx)),
    },
    ToolDescriptor {
        name: "ghostcode_inbox_mark_all_read",
        description: "Mark all messages as read.",
        schema: inbox_mark_all_read::schema,
        execute: |args, ctx| Box::pin(inbox_mark_all_read::execute_owned(args, ctx)),
    },
    ToolDescriptor {
        name: "ghostcode_actor_list",
        description: "List all actors in the current group.",
        schema: actor_list::schema,
        execute: |args, ctx| Box::pin(actor_list::execute_owned(args, ctx)),
    },
    ToolDescriptor {
        name: "ghostcode_actor_start",
        description: "Start an actor.",
        schema: actor_start::schema,
        execute: |args, ctx| Box::pin(actor_start::execute_owned(args, ctx)),
    },
    ToolDescriptor {
        name: "ghostcode_actor_stop",
        description: "Stop an actor.",
        schema: actor_stop::schema,
        execute: |args, ctx| Box::pin(actor_stop::execute_owned(args, ctx)),
    },
    ToolDescriptor {
        name: "ghostcode_group_info",
        description: "Get information about the current group.",
        schema: group_info::schema,
        execute: |args, ctx| Box::pin(group_info::execute_owned(args, ctx)),
    },
    ToolDescriptor {
        name: "ghostcode_route_task",
        description: "Route a task to an actor.",
        schema: route_task::schema,
        execute: |args, ctx| Box::pin(route_task::execute_owned(args, ctx)),
    },
    ToolDescriptor {
        name: "ghostcode_route_status",
        description: "Get the status of a routed task.",
        schema: route_status::schema,
        execute: |args, ctx| Box::pin(route_status::execute_owned(args, ctx)),
    },
    ToolDescriptor {
        name: "ghostcode_route_cancel",
        description: "Cancel a routed task.",
        schema: route_cancel::schema,
        execute: |args, ctx| Box::pin(route_cancel::execute_owned(args, ctx)),
    },
    // --------------------------------------------------------
    // 新增 5 个工具
    // --------------------------------------------------------
    ToolDescriptor {
        name: "ghostcode_group_list",
        description: "List all available working groups.",
        schema: group_list::schema,
        execute: |args, ctx| Box::pin(group_list::execute_owned(args, ctx)),
    },
    ToolDescriptor {
        name: "ghostcode_dashboard_snapshot",
        description: "Get a dashboard snapshot of the current group.",
        schema: dashboard_snapshot::schema,
        execute: |args, ctx| Box::pin(dashboard_snapshot::execute_owned(args, ctx)),
    },
    ToolDescriptor {
        name: "ghostcode_verification_status",
        description: "Get the verification status of the current group.",
        schema: verification_status::schema,
        execute: |args, ctx| Box::pin(verification_status::execute_owned(args, ctx)),
    },
    ToolDescriptor {
        name: "ghostcode_skill_list",
        description: "List available skills in the current group.",
        schema: skill_list::schema,
        execute: |args, ctx| Box::pin(skill_list::execute_owned(args, ctx)),
    },
    ToolDescriptor {
        name: "ghostcode_team_skill_list",
        description: "List skills aggregated across all groups.",
        schema: team_skill_list::schema,
        execute: |args, ctx| Box::pin(team_skill_list::execute_owned(args, ctx)),
    },
];

// ============================================================
// 注册表公共 API
// ============================================================

/// 获取全部工具注册表
///
/// 由 server.rs 的 tools/list 处理器调用，返回所有工具描述符
pub fn registry() -> &'static [ToolDescriptor] {
    REGISTRY
}

/// 按名称查找工具描述符
///
/// 返回 None 表示工具不存在，调用方应返回 invalid_param 错误
pub fn find_tool(name: &str) -> Option<&'static ToolDescriptor> {
    REGISTRY.iter().find(|d| d.name == name)
}

// ============================================================
// 工具 Schema 列表（兼容旧 API，用于 tools/list 响应）
// ============================================================

/// 返回所有工具的 schema 定义列表
///
/// 由 server.rs 的 tools/list 处理器调用
/// 参考: cccc/src/cccc/ports/mcp/toolspecs.py - MCP_TOOLS 结构
pub fn all_tool_schemas() -> Vec<serde_json::Value> {
    REGISTRY.iter().map(|d| (d.schema)()).collect()
}

// ============================================================
// 工具分发函数
// 由 server.rs 的 tools/call 处理器调用
// ============================================================

/// 根据工具名分发工具调用
///
/// 业务逻辑：
/// 1. 通过 find_tool 在注册表中查找工具描述符
/// 2. 调用该描述符的 execute 函数
/// 3. 返回工具结果或错误
pub async fn dispatch_tool(
    tool_name: &str,
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    match find_tool(tool_name) {
        Some(descriptor) => (descriptor.execute)(args.clone(), ctx.clone()).await,
        None => Err(ToolError::InvalidParam {
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
    // 测试 1: all_tool_schemas 返回 16 个工具定义
    // --------------------------------------------------------
    #[test]
    fn all_schemas_returns_16_tools() {
        let schemas = all_tool_schemas();
        assert_eq!(schemas.len(), 16, "必须恰好返回 16 个工具定义");

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
    // 测试 10: 验证工具名称列表与注册表一致
    // --------------------------------------------------------
    #[test]
    fn all_schema_names_match_dispatch_routes() {
        let schemas = all_tool_schemas();
        let actual_names: Vec<&str> = schemas
            .iter()
            .filter_map(|s| s.get("name").and_then(|n| n.as_str()))
            .collect();

        // 每个注册表条目都能在 all_tool_schemas() 中找到
        for descriptor in registry() {
            assert!(
                actual_names.contains(&descriptor.name),
                "工具 '{}' 必须出现在 all_tool_schemas() 中",
                descriptor.name
            );
        }
    }
}
