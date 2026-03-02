# Team Plan: T15 核心 MCP 工具实现

## 概述

为 `ghostcode-mcp` crate 实现 8 个核心 MCP 工具函数，每个工具遵循统一模式：

```
解析 MCP 参数 → 从环境变量读取身份 → 构造 DaemonRequest → call_daemon() → 格式化响应 → 返回 MCP content
```

产出文件：`crates/ghostcode-mcp/src/tools/` 目录下的 8 个工具模块 + 1 个模块索引文件，
以及对 `server.rs` 的更新（替换静态占位工具列表为真实工具定义，并更新 `tools/call` 路由）。

**前置依赖**：
- T14 完成：`crates/ghostcode-mcp/src/server.rs` 已实现（`call_daemon` 函数、`serve_stdio` 主循环）
- T07 完成：`crates/ghostcode-types/src/ipc.rs` 已实现（`DaemonRequest` / `DaemonResponse`）

---

## Codex 分析摘要

Codex CLI 不可用，由 Claude 自行分析。

---

## Gemini 分析摘要

批量计划生成模式，跳过多模型分析。

---

## 技术方案

### 参考实现溯源

| 参考文件 | 对应工具 | 关键逻辑 |
|---------|---------|---------|
| `cccc/src/cccc/ports/mcp/handlers/cccc_messaging.py:15-61` | `ghostcode_message_send` | `op: "send"` 参数构造 |
| `cccc/src/cccc/ports/mcp/handlers/cccc_core.py:201-203` | `ghostcode_inbox_list` | `op: "inbox_list"` |
| `cccc/src/cccc/ports/mcp/handlers/cccc_core.py:207-213` | `ghostcode_inbox_mark_read` | `op: "inbox_mark_read"` + 空参数校验 |
| `cccc/src/cccc/ports/mcp/handlers/cccc_core.py:216-219` | `ghostcode_inbox_mark_all_read` | `op: "inbox_mark_all_read"` |
| `cccc/src/cccc/ports/mcp/handlers/cccc_group_actor.py:97-101` | `ghostcode_actor_list` | `op: "actor_list"` + actors 字段提取 |
| `cccc/src/cccc/ports/mcp/handlers/cccc_group_actor.py:142-147` | `ghostcode_actor_start` | `op: "actor_start"` |
| `cccc/src/cccc/ports/mcp/handlers/cccc_group_actor.py:150-155` | `ghostcode_actor_stop` | `op: "actor_stop"` |
| `cccc/src/cccc/ports/mcp/handlers/cccc_group_actor.py:63-67` | `ghostcode_group_info` | `op: "group_show"` + group 字段提取 |
| `cccc/src/cccc/ports/mcp/toolspecs.py:116-136` | `ghostcode_message_send` inputSchema | 参数定义（text 为 required）|
| `cccc/src/cccc/ports/mcp/toolspecs.py:80-94` | `ghostcode_inbox_list` inputSchema | limit / kind_filter |
| `cccc/src/cccc/ports/mcp/toolspecs.py:197-218` | `ghostcode_actor_*` inputSchema | actor_id 参数 |

### 架构设计

```
tools/call (server.rs)
    |
    v
dispatch_tool(tool_name, arguments, group_id, actor_id, daemon_addr)
    |
    ├── "ghostcode_message_send"      -> tools/message_send.rs::execute()
    ├── "ghostcode_inbox_list"        -> tools/inbox_list.rs::execute()
    ├── "ghostcode_inbox_mark_read"   -> tools/inbox_mark_read.rs::execute()
    ├── "ghostcode_inbox_mark_all_read" -> tools/inbox_mark_all_read.rs::execute()
    ├── "ghostcode_actor_list"        -> tools/actor_list.rs::execute()
    ├── "ghostcode_actor_start"       -> tools/actor_start.rs::execute()
    ├── "ghostcode_actor_stop"        -> tools/actor_stop.rs::execute()
    └── "ghostcode_group_info"        -> tools/group_info.rs::execute()
```

### 统一工具函数签名

每个工具模块暴露两个公共符号：

```rust
// 工具输入 Schema（用于 tools/list 响应）
pub fn schema() -> serde_json::Value;

// 工具执行函数
pub async fn execute(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError>;
```

### ToolContext 和 ToolError

```rust
// crates/ghostcode-mcp/src/tools/mod.rs

/// 工具调用上下文（由 server.rs 构造后传入工具函数）
///
/// 包含执行工具所需的环境信息，避免每次从环境变量重复读取
pub struct ToolContext {
    /// Daemon Unix Socket 路径
    pub daemon_addr: std::path::PathBuf,
    /// GhostCode 工作组 ID（从 GHOSTCODE_GROUP_ID 读取）
    pub group_id: String,
    /// Actor ID（从 GHOSTCODE_ACTOR_ID 读取）
    pub actor_id: String,
}

/// 工具调用错误类型
///
/// 区分参数验证错误和 Daemon 调用错误，便于上层构造正确的错误响应
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// 缺少必填参数
    #[error("missing required parameter: {0}")]
    MissingParam(String),

    /// 参数类型或值无效
    #[error("invalid parameter '{name}': {reason}")]
    InvalidParam { name: String, reason: String },

    /// Daemon 调用失败（网络、协议、业务错误）
    #[error("daemon error: {0}")]
    Daemon(#[from] anyhow::Error),
}
```

### MCP tools/call 响应格式

工具执行成功时，MCP 规范要求将结果包装为 content 数组：

```json
{
  "content": [
    {
      "type": "text",
      "text": "<JSON 格式的工具结果>"
    }
  ]
}
```

工具执行失败（`ToolError`）时，以 `isError: true` 返回（不使用 JSON-RPC error，保持 MCP 规范）：

```json
{
  "content": [
    {
      "type": "text",
      "text": "{\"error\": {\"code\": \"missing_param\", \"message\": \"...\"}}"
    }
  ],
  "isError": true
}
```

### 禁止降级策略

- Daemon 连接失败必须返回 `ToolError::Daemon`，不得返回空结果
- 必填参数缺失必须返回 `ToolError::MissingParam`，不得使用默认值替代
- Daemon 返回 `ok: false` 必须返回错误，不得静默忽略

---

## 子任务列表

---

### Task 1: 创建 tools/mod.rs — ToolContext / ToolError / dispatch_tool

**文件**: `crates/ghostcode-mcp/src/tools/mod.rs`
**依赖**: 无（可最先开始）
**可并行**: 是（与 Task 2-9 并行开始，各工具模块依赖此文件的类型定义）

#### 完整实现内容

```rust
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

use std::path::Path;
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
///
/// # 参数
/// - tool_name: MCP tools/call 请求中的 name 字段
/// - args: MCP tools/call 请求中的 arguments 字段
/// - ctx: 工具执行上下文（包含 daemon_addr / group_id / actor_id）
///
/// # 返回
/// - Ok(serde_json::Value): 工具执行成功的结果
/// - Err(ToolError): 参数错误或 Daemon 调用失败
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
        _ => Err(ToolError::InvalidParam {
            name: "tool_name".to_string(),
            reason: format!("unknown tool: {}", tool_name),
        }),
    }
}
```

#### 验收标准
- `ToolContext::from_env()` 可正常构造
- `ToolError::MissingParam("event_id".into()).to_mcp_error_json()` 返回包含 `"code":"missing_param"` 的 JSON
- `all_tool_schemas()` 返回恰好 8 个元素的 Vec
- `dispatch_tool("unknown_tool", ...)` 返回 `ToolError::InvalidParam`

---

### Task 2: 创建 tools/message_send.rs

**文件**: `crates/ghostcode-mcp/src/tools/message_send.rs`
**依赖**: Task 1（ToolContext / ToolError 类型）
**可并行**: 与 Task 3-9 并行

#### 参考溯源
- 参考: `cccc/src/cccc/ports/mcp/handlers/cccc_messaging.py:15-61` — `message_send` 函数，`op: "send"` 参数构造
- 参考: `cccc/src/cccc/ports/mcp/toolspecs.py:116-136` — inputSchema，`text` 为 required

#### 完整实现内容

```rust
//! ghostcode_message_send 工具实现
//!
//! 发送消息给工作组内的 Agent
//! 对应 Daemon op: "send"
//!
//! 参考: cccc/src/cccc/ports/mcp/handlers/cccc_messaging.py:15-61
//! 参考: cccc/src/cccc/ports/mcp/toolspecs.py:116-136
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

/// 返回工具的 MCP inputSchema 定义
///
/// 遵循 JSON Schema 规范，供 tools/list 响应使用
/// text 为必填参数，其余可选
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_message_send",
        "description": "Send a visible chat message to the group or specific actors.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Message content (required)"
                },
                "to": {
                    "description": "Target actor IDs. Empty = broadcast to all. String or array of strings.",
                    "anyOf": [
                        {"type": "string"},
                        {"type": "array", "items": {"type": "string"}}
                    ]
                },
                "reply_to": {
                    "type": "string",
                    "description": "Event ID to reply to (optional)"
                },
                "priority": {
                    "type": "string",
                    "enum": ["normal", "attention"],
                    "default": "normal"
                }
            },
            "required": ["text"]
        }
    })
}

/// 执行 ghostcode_message_send 工具
///
/// 业务逻辑：
/// 1. 从 args 提取 text（必填），缺失返回 MissingParam 错误
/// 2. 提取可选参数 to / reply_to / priority
/// 3. 将 to 统一为 Vec<String>（支持字符串或数组两种输入格式）
/// 4. 构造 DaemonRequest { op: "send", args: {...} }
/// 5. 调用 call_daemon，返回 Daemon 的 result
///
/// # 参数
/// - args: MCP tools/call 的 arguments 字段
/// - ctx: 工具上下文（group_id / actor_id / daemon_addr）
///
/// # 返回
/// - Ok({"event_id": "..."}): 发送成功，包含消息事件 ID
/// - Err(ToolError::MissingParam): text 未提供
/// - Err(ToolError::Daemon): Daemon 调用失败
pub async fn execute(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    // ============================================
    // 第一步：提取必填参数 text
    // 参考: cccc 中所有工具都严格验证必填字段后才调用 daemon
    // ============================================
    let text = args
        .get("text")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ToolError::MissingParam("text".to_string()))?;

    // ============================================
    // 第二步：提取可选参数
    // to 支持字符串（单个）或数组（多个）两种格式
    // 参考: cccc toolspecs.py:125-129 anyOf 定义
    // ============================================
    let to: Vec<String> = match args.get("to") {
        Some(serde_json::Value::String(s)) => {
            // 单个字符串目标
            if s.is_empty() { vec![] } else { vec![s.clone()] }
        }
        Some(serde_json::Value::Array(arr)) => {
            // 数组：过滤非字符串元素
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .filter(|s| !s.is_empty())
                .collect()
        }
        _ => vec![],
    };

    // reply_to: 可选的回复目标事件 ID
    let reply_to = args
        .get("reply_to")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    // priority: 只允许 "normal" 或 "attention"，其他值视为 "normal"
    let priority = {
        let p = args
            .get("priority")
            .and_then(|v| v.as_str())
            .unwrap_or("normal");
        match p {
            "normal" | "attention" => p.to_string(),
            _ => "normal".to_string(),
        }
    };

    // ============================================
    // 第三步：构造 Daemon 请求
    // op 根据是否有 reply_to 决定使用 "send" 还是 "reply"
    // 参考: cccc cccc_messaging.py:48-61 send args 结构
    // ============================================
    let (op, daemon_args) = if reply_to.is_empty() {
        (
            "send",
            serde_json::json!({
                "group_id": ctx.group_id,
                "text": text,
                "by": ctx.actor_id,
                "to": to,
                "path": "",
                "priority": priority,
                "reply_required": false
            }),
        )
    } else {
        (
            "reply",
            serde_json::json!({
                "group_id": ctx.group_id,
                "text": text,
                "by": ctx.actor_id,
                "reply_to": reply_to,
                "to": to,
                "priority": priority,
                "reply_required": false
            }),
        )
    };

    let req = DaemonRequest::new(op, daemon_args);

    // ============================================
    // 第四步：调用 Daemon 并返回结果
    // 失败时直接传播 ToolError::Daemon，禁止降级
    // ============================================
    let result = call_daemon(&ctx.daemon_addr, req).await?;
    Ok(result)
}
```

#### 验收标准
- `schema()` 返回包含 `"name":"ghostcode_message_send"` 和 `"required":["text"]` 的 JSON
- `execute({"text": "hello"}, ctx)` 构造正确的 DaemonRequest 并调用 call_daemon
- `execute({}, ctx)` 返回 `ToolError::MissingParam("text")`
- `to` 字段为字符串时，转为单元素 Vec；为数组时逐一提取

---

### Task 3: 创建 tools/inbox_list.rs

**文件**: `crates/ghostcode-mcp/src/tools/inbox_list.rs`
**依赖**: Task 1
**可并行**: 与 Task 2、4-9 并行

#### 参考溯源
- 参考: `cccc/src/cccc/ports/mcp/handlers/cccc_core.py:201-203` — `inbox_list` 函数，`op: "inbox_list"`
- 参考: `cccc/src/cccc/ports/mcp/toolspecs.py:80-94` — inputSchema，limit 默认 50

#### 完整实现内容

```rust
//! ghostcode_inbox_list 工具实现
//!
//! 列出当前 Actor 的未读消息
//! 对应 Daemon op: "inbox_list"
//!
//! 参考: cccc/src/cccc/ports/mcp/handlers/cccc_core.py:201-203
//! 参考: cccc/src/cccc/ports/mcp/toolspecs.py:80-94
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

/// 返回工具的 MCP inputSchema 定义
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_inbox_list",
        "description": "List unread inbox messages for the current actor.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "default": 50,
                    "minimum": 1,
                    "maximum": 1000,
                    "description": "Maximum number of messages to return"
                }
            },
            "required": []
        }
    })
}

/// 执行 ghostcode_inbox_list 工具
///
/// 业务逻辑：
/// 1. 从 args 提取可选 limit（默认 50，范围 1-1000）
/// 2. 构造 DaemonRequest { op: "inbox_list", args: {...} }
/// 3. 调用 call_daemon，返回 Daemon 的 result（包含 messages 数组）
///
/// # 参数
/// - args: MCP tools/call 的 arguments 字段
/// - ctx: 工具上下文
///
/// # 返回
/// - Ok({"messages": [...]}): 未读消息列表
/// - Err(ToolError::Daemon): Daemon 调用失败
pub async fn execute(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    // 提取可选 limit 参数，范围限制在 1-1000
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|n| n.clamp(1, 1000))
        .unwrap_or(50);

    let req = DaemonRequest::new(
        "inbox_list",
        serde_json::json!({
            "group_id": ctx.group_id,
            "actor_id": ctx.actor_id,
            "by": ctx.actor_id,
            "limit": limit,
            // kind_filter 固定为 "all"，T15 阶段不暴露此参数
            "kind_filter": "all"
        }),
    );

    let result = call_daemon(&ctx.daemon_addr, req).await?;
    Ok(result)
}
```

#### 验收标准
- `schema()` 返回 `"name":"ghostcode_inbox_list"`，required 为空数组
- `execute({}, ctx)` 使用默认 limit=50 构造请求
- `execute({"limit": 10}, ctx)` 使用 limit=10 构造请求
- `execute({"limit": 9999}, ctx)` 将 limit 限制为 1000

---

### Task 4: 创建 tools/inbox_mark_read.rs

**文件**: `crates/ghostcode-mcp/src/tools/inbox_mark_read.rs`
**依赖**: Task 1
**可并行**: 与 Task 2、3、5-9 并行

#### 参考溯源
- 参考: `cccc/src/cccc/ports/mcp/handlers/cccc_core.py:207-213` — `inbox_mark_read`，空参数校验 + `op: "inbox_mark_read"`

#### 完整实现内容

```rust
//! ghostcode_inbox_mark_read 工具实现
//!
//! 将指定事件 ID 的消息标记为已读
//! 对应 Daemon op: "inbox_mark_read"
//!
//! 参考: cccc/src/cccc/ports/mcp/handlers/cccc_core.py:207-213
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

/// 返回工具的 MCP inputSchema 定义
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_inbox_mark_read",
        "description": "Mark a specific message as read by its event ID.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "event_id": {
                    "type": "string",
                    "description": "Event ID of the message to mark as read (required)"
                }
            },
            "required": ["event_id"]
        }
    })
}

/// 执行 ghostcode_inbox_mark_read 工具
///
/// 业务逻辑：
/// 1. 从 args 提取必填 event_id，空字符串视为缺失
/// 2. 构造 DaemonRequest { op: "inbox_mark_read", args: {...} }
/// 3. 调用 call_daemon，返回结果
///
/// # 参数
/// - args: MCP tools/call 的 arguments 字段
/// - ctx: 工具上下文
///
/// # 返回
/// - Ok({"ok": true}): 标记成功
/// - Err(ToolError::MissingParam): event_id 未提供或为空字符串
/// - Err(ToolError::Daemon): Daemon 调用失败
pub async fn execute(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    // ============================================
    // 提取并验证必填参数 event_id
    // 空字符串视为缺失（与 cccc 行为一致）
    // 参考: cccc cccc_core.py:208-210 - 空参数检查
    // ============================================
    let event_id = args
        .get("event_id")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ToolError::MissingParam("event_id".to_string()))?;

    let req = DaemonRequest::new(
        "inbox_mark_read",
        serde_json::json!({
            "group_id": ctx.group_id,
            "actor_id": ctx.actor_id,
            "event_id": event_id,
            "by": ctx.actor_id
        }),
    );

    let result = call_daemon(&ctx.daemon_addr, req).await?;
    Ok(result)
}
```

#### 验收标准
- `schema()` 返回 `"required":["event_id"]`
- `execute({}, ctx)` 返回 `ToolError::MissingParam("event_id")`
- `execute({"event_id": ""}, ctx)` 返回 `ToolError::MissingParam("event_id")`（空字符串视为缺失）
- `execute({"event_id": "evt-123"}, ctx)` 正常构造 Daemon 请求

---

### Task 5: 创建 tools/inbox_mark_all_read.rs

**文件**: `crates/ghostcode-mcp/src/tools/inbox_mark_all_read.rs`
**依赖**: Task 1
**可并行**: 与 Task 2-4、6-9 并行

#### 参考溯源
- 参考: `cccc/src/cccc/ports/mcp/handlers/cccc_core.py:216-219` — `inbox_mark_all_read`，无参数，`op: "inbox_mark_all_read"`

#### 完整实现内容

```rust
//! ghostcode_inbox_mark_all_read 工具实现
//!
//! 将当前 Actor 的所有未读消息标记为已读
//! 对应 Daemon op: "inbox_mark_all_read"
//!
//! 参考: cccc/src/cccc/ports/mcp/handlers/cccc_core.py:216-219
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

/// 返回工具的 MCP inputSchema 定义
///
/// 此工具无需输入参数，inputSchema 为空对象
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_inbox_mark_all_read",
        "description": "Mark all unread messages as read for the current actor.",
        "inputSchema": {
            "type": "object",
            "properties": {},
            "required": []
        }
    })
}

/// 执行 ghostcode_inbox_mark_all_read 工具
///
/// 业务逻辑：
/// 1. 无需解析参数（此工具无输入参数）
/// 2. 构造 DaemonRequest { op: "inbox_mark_all_read", args: {...} }
/// 3. 调用 call_daemon，返回结果
///
/// # 参数
/// - args: MCP tools/call 的 arguments 字段（此工具忽略）
/// - ctx: 工具上下文
///
/// # 返回
/// - Ok({"ok": true}): 标记成功
/// - Err(ToolError::Daemon): Daemon 调用失败
pub async fn execute(
    _args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    let req = DaemonRequest::new(
        "inbox_mark_all_read",
        serde_json::json!({
            "group_id": ctx.group_id,
            "actor_id": ctx.actor_id,
            // kind_filter 固定为 "all"
            "kind_filter": "all",
            "by": ctx.actor_id
        }),
    );

    let result = call_daemon(&ctx.daemon_addr, req).await?;
    Ok(result)
}
```

#### 验收标准
- `schema()` 返回 `"required":[]`，properties 为空对象
- `execute({}, ctx)` 构造正确的 Daemon 请求（包含 group_id / actor_id / kind_filter）
- 任何 args 输入都不影响执行结果（参数被忽略）

---

### Task 6: 创建 tools/actor_list.rs

**文件**: `crates/ghostcode-mcp/src/tools/actor_list.rs`
**依赖**: Task 1
**可并行**: 与 Task 2-5、7-9 并行

#### 参考溯源
- 参考: `cccc/src/cccc/ports/mcp/handlers/cccc_group_actor.py:97-101` — `actor_list` 函数，`op: "actor_list"` + `include_unread: true`

#### 完整实现内容

```rust
//! ghostcode_actor_list 工具实现
//!
//! 列出工作组内所有 Actor 信息
//! 对应 Daemon op: "actor_list"
//!
//! 参考: cccc/src/cccc/ports/mcp/handlers/cccc_group_actor.py:97-101
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

/// 返回工具的 MCP inputSchema 定义
///
/// 此工具无需输入参数（group_id 来自环境变量）
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_actor_list",
        "description": "List all actors in the current working group.",
        "inputSchema": {
            "type": "object",
            "properties": {},
            "required": []
        }
    })
}

/// 执行 ghostcode_actor_list 工具
///
/// 业务逻辑：
/// 1. 无需解析参数（group_id 来自 ctx）
/// 2. 构造 DaemonRequest { op: "actor_list", args: {group_id, include_unread: true} }
/// 3. 调用 call_daemon，返回包含 actors 数组的结果
///
/// # 参数
/// - args: MCP tools/call 的 arguments 字段（此工具忽略）
/// - ctx: 工具上下文
///
/// # 返回
/// - Ok({"actors": [...]}): Actor 列表
/// - Err(ToolError::Daemon): Daemon 调用失败
pub async fn execute(
    _args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    // include_unread: true - 与 cccc 保持一致，返回未读消息计数
    // 参考: cccc cccc_group_actor.py:99 - include_unread=True
    let req = DaemonRequest::new(
        "actor_list",
        serde_json::json!({
            "group_id": ctx.group_id,
            "include_unread": true
        }),
    );

    let result = call_daemon(&ctx.daemon_addr, req).await?;
    Ok(result)
}
```

#### 验收标准
- `schema()` 返回 `"name":"ghostcode_actor_list"`，required 为空
- `execute({}, ctx)` 构造包含 `"include_unread": true` 的 Daemon 请求
- group_id 来自 ctx，不从 args 读取

---

### Task 7: 创建 tools/actor_start.rs

**文件**: `crates/ghostcode-mcp/src/tools/actor_start.rs`
**依赖**: Task 1
**可并行**: 与 Task 2-6、8-9 并行

#### 参考溯源
- 参考: `cccc/src/cccc/ports/mcp/handlers/cccc_group_actor.py:142-147` — `actor_start` 函数，`op: "actor_start"` + actor_id 必填

#### 完整实现内容

```rust
//! ghostcode_actor_start 工具实现
//!
//! 启动指定 Actor（设置 enabled=true）
//! 对应 Daemon op: "actor_start"
//!
//! 参考: cccc/src/cccc/ports/mcp/handlers/cccc_group_actor.py:142-147
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

/// 返回工具的 MCP inputSchema 定义
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_actor_start",
        "description": "Start an actor (set enabled=true). Only foreman can start actors.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "actor_id": {
                    "type": "string",
                    "description": "ID of the actor to start (required)"
                }
            },
            "required": ["actor_id"]
        }
    })
}

/// 执行 ghostcode_actor_start 工具
///
/// 业务逻辑：
/// 1. 从 args 提取必填 actor_id，空字符串视为缺失
/// 2. 构造 DaemonRequest { op: "actor_start", args: {group_id, actor_id, by} }
/// 3. 调用 call_daemon，返回结果
///
/// # 参数
/// - args: MCP tools/call 的 arguments 字段
/// - ctx: 工具上下文（by = ctx.actor_id 表示调用者）
///
/// # 返回
/// - Ok({"ok": true}): 启动成功
/// - Err(ToolError::MissingParam): actor_id 未提供
/// - Err(ToolError::Daemon): Daemon 调用失败（权限不足等）
pub async fn execute(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    // 提取必填 actor_id
    let actor_id = args
        .get("actor_id")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ToolError::MissingParam("actor_id".to_string()))?;

    // by: 操作发起者（当前 Actor）
    // 参考: cccc cccc_group_actor.py:145 - by 参数
    let req = DaemonRequest::new(
        "actor_start",
        serde_json::json!({
            "group_id": ctx.group_id,
            "actor_id": actor_id,
            "by": ctx.actor_id
        }),
    );

    let result = call_daemon(&ctx.daemon_addr, req).await?;
    Ok(result)
}
```

#### 验收标准
- `schema()` 返回 `"required":["actor_id"]`
- `execute({}, ctx)` 返回 `ToolError::MissingParam("actor_id")`
- `execute({"actor_id": "peer-1"}, ctx)` 构造包含 `"by": ctx.actor_id` 的 Daemon 请求

---

### Task 8: 创建 tools/actor_stop.rs

**文件**: `crates/ghostcode-mcp/src/tools/actor_stop.rs`
**依赖**: Task 1
**可并行**: 与 Task 2-7、9 并行

#### 参考溯源
- 参考: `cccc/src/cccc/ports/mcp/handlers/cccc_group_actor.py:150-155` — `actor_stop` 函数，`op: "actor_stop"`

#### 完整实现内容

```rust
//! ghostcode_actor_stop 工具实现
//!
//! 停止指定 Actor（设置 enabled=false）
//! 对应 Daemon op: "actor_stop"
//!
//! 参考: cccc/src/cccc/ports/mcp/handlers/cccc_group_actor.py:150-155
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

/// 返回工具的 MCP inputSchema 定义
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_actor_stop",
        "description": "Stop an actor (set enabled=false). Foreman can stop any actor; peer can only stop themselves.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "actor_id": {
                    "type": "string",
                    "description": "ID of the actor to stop (required)"
                }
            },
            "required": ["actor_id"]
        }
    })
}

/// 执行 ghostcode_actor_stop 工具
///
/// 业务逻辑：
/// 1. 从 args 提取必填 actor_id，空字符串视为缺失
/// 2. 构造 DaemonRequest { op: "actor_stop", args: {group_id, actor_id, by} }
/// 3. 调用 call_daemon，权限校验由 Daemon 执行
///
/// # 参数
/// - args: MCP tools/call 的 arguments 字段
/// - ctx: 工具上下文
///
/// # 返回
/// - Ok({"ok": true}): 停止成功
/// - Err(ToolError::MissingParam): actor_id 未提供
/// - Err(ToolError::Daemon): Daemon 调用失败（权限不足等）
pub async fn execute(
    args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    let actor_id = args
        .get("actor_id")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ToolError::MissingParam("actor_id".to_string()))?;

    let req = DaemonRequest::new(
        "actor_stop",
        serde_json::json!({
            "group_id": ctx.group_id,
            "actor_id": actor_id,
            "by": ctx.actor_id
        }),
    );

    let result = call_daemon(&ctx.daemon_addr, req).await?;
    Ok(result)
}
```

#### 验收标准
- `schema()` 与 actor_start.rs 结构对称，只有 name 和 description 不同
- `execute({}, ctx)` 返回 `ToolError::MissingParam("actor_id")`
- `execute({"actor_id": "peer-1"}, ctx)` 构造 op 为 `"actor_stop"` 的 Daemon 请求

---

### Task 9: 创建 tools/group_info.rs

**文件**: `crates/ghostcode-mcp/src/tools/group_info.rs`
**依赖**: Task 1
**可并行**: 与 Task 2-8 并行

#### 参考溯源
- 参考: `cccc/src/cccc/ports/mcp/handlers/cccc_group_actor.py:63-67` — `group_info` 函数，`op: "group_show"` + group 字段提取

#### 完整实现内容

```rust
//! ghostcode_group_info 工具实现
//!
//! 获取当前工作组的基本信息
//! 对应 Daemon op: "group_show"
//!
//! 参考: cccc/src/cccc/ports/mcp/handlers/cccc_group_actor.py:63-67
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_types::ipc::DaemonRequest;
use crate::server::call_daemon;
use super::{ToolContext, ToolError};

/// 返回工具的 MCP inputSchema 定义
///
/// 此工具无需输入参数（group_id 来自环境变量）
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "ghostcode_group_info",
        "description": "Get information about the current working group.",
        "inputSchema": {
            "type": "object",
            "properties": {},
            "required": []
        }
    })
}

/// 执行 ghostcode_group_info 工具
///
/// 业务逻辑：
/// 1. 无需解析参数（group_id 来自 ctx）
/// 2. 构造 DaemonRequest { op: "group_show", args: {group_id} }
/// 3. 调用 call_daemon，返回 group 信息
///
/// # 参数
/// - args: MCP tools/call 的 arguments 字段（此工具忽略）
/// - ctx: 工具上下文
///
/// # 返回
/// - Ok({"group": {...}}): 工作组信息
/// - Err(ToolError::Daemon): Daemon 调用失败（group 不存在等）
pub async fn execute(
    _args: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<serde_json::Value, ToolError> {
    let req = DaemonRequest::new(
        "group_show",
        serde_json::json!({
            "group_id": ctx.group_id
        }),
    );

    let result = call_daemon(&ctx.daemon_addr, req).await?;
    Ok(result)
}
```

#### 验收标准
- `schema()` 返回 `"name":"ghostcode_group_info"`，required 为空
- `execute({}, ctx)` 构造 op 为 `"group_show"` 的 Daemon 请求，args 包含 group_id
- group_id 来自 ctx，不从 args 读取

---

### Task 10: 更新 server.rs — 集成真实工具列表和分发逻辑

**文件**: `crates/ghostcode-mcp/src/server.rs`（修改已有文件）
**依赖**: Task 1-9 全部完成
**可并行**: 否（必须在工具模块全部就绪后执行）

#### 需要修改的内容

**修改点 1**: 将 `static_tool_list()` 替换为调用 `tools::all_tool_schemas()`

找到 `server.rs` 中的 `static_tool_list()` 函数，整体删除并替换。

删除以下函数（T14 阶段的占位实现）：
```rust
fn static_tool_list() -> serde_json::Value {
    serde_json::json!([...8 个占位工具...])
}
```

无需添加新函数——在 `tools/list` 处理分支中直接调用 `crate::tools::all_tool_schemas()`。

**修改点 2**: 更新 `tools/list` 处理分支

将原来：
```rust
"tools/list" => {
    let tools = static_tool_list();
    let tools_arr = tools.as_array().cloned().unwrap_or_default();
    ...
}
```

替换为：
```rust
// --------------------------------------------------
// tools/list: 返回工具列表（来自 tools::all_tool_schemas()）
// --------------------------------------------------
"tools/list" => {
    let tools_arr = crate::tools::all_tool_schemas();
    let total = tools_arr.len();

    // 解析分页参数 cursor（字符串形式的偏移量）
    let cursor: usize = params
        .get("cursor")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let limit: usize = params
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|n| (n as usize).clamp(1, 200))
        .unwrap_or(100);

    let page: Vec<_> = tools_arr
        .into_iter()
        .skip(cursor)
        .take(limit)
        .collect();

    let next_cursor = if cursor + limit < total {
        serde_json::Value::String((cursor + limit).to_string())
    } else {
        serde_json::Value::String(String::new())
    };

    let result = serde_json::json!({
        "tools": page,
        "nextCursor": next_cursor
    });
    Some(JsonRpcResponse::ok(id, result))
}
```

**修改点 3**: 更新 `tools/call` 处理分支

将原来的手动 DaemonRequest 构造逻辑替换为调用 `dispatch_tool`：

```rust
// --------------------------------------------------
// tools/call: 工具调用，通过 dispatch_tool 分发
// 参考: cccc/src/cccc/ports/mcp/main.py:163-222
// --------------------------------------------------
"tools/call" => {
    let tool_name = params
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or(serde_json::Value::Object(Default::default()));

    // 构造工具上下文（身份信息来自环境变量）
    let ctx = crate::tools::ToolContext::from_env(daemon_addr);

    // 分发到对应工具模块
    match crate::tools::dispatch_tool(&tool_name, &arguments, &ctx).await {
        Ok(result) => {
            // MCP tools/call 成功响应格式
            let content = serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|_| result.to_string())
                }]
            });
            Some(JsonRpcResponse::ok(id, content))
        }
        Err(e) => {
            // 工具调用失败：MCP 规范以 isError:true 返回
            // 禁止静默降级，错误信息必须暴露给调用者
            let content = serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": e.to_mcp_error_json()
                }],
                "isError": true
            });
            Some(JsonRpcResponse::ok(id, content))
        }
    }
}
```

**修改点 4**: 将 `call_daemon` 函数改为 `pub(crate)`

工具模块需要调用 `call_daemon`，将其可见性从私有改为 `pub(crate)`：

```rust
// 将原来的 async fn call_daemon 改为：
pub(crate) async fn call_daemon(
    daemon_addr: &Path,
    req: DaemonRequest,
) -> Result<serde_json::Value> {
    // ... 实现不变
}
```

#### 验收标准
- `tools/list` 响应中包含 8 个真实工具定义（不再是占位）
- `tools/call ghostcode_message_send {"text":"hi"}` 能到达 message_send::execute
- `tools/call ghostcode_message_send {}` 返回 `isError: true` 响应
- `call_daemon` 可被 tools 子模块访问

---

### Task 11: 更新 lib.rs — 导出 tools 模块

**文件**: `crates/ghostcode-mcp/src/lib.rs`（修改已有文件）
**依赖**: Task 1（tools/mod.rs 存在）、Task 10（server.rs 更新完成）
**可并行**: 否

#### 完整实现内容

将 `lib.rs` 内容替换为：

```rust
//! GhostCode MCP Server
//!
//! 将 GhostCode 功能暴露为 MCP 工具供 Claude Code 调用
//! 实现 stdio JSON-RPC 2.0 服务器协议，包含 8 个核心工具
//!
//! @author Atlas.oi
//! @date 2026-03-01

pub mod jsonrpc;
pub mod server;
pub mod tools;

// 重新导出公共 API
pub use server::serve_stdio;
pub use tools::{ToolContext, ToolError};
```

#### 验收标准
- `use ghostcode_mcp::tools::ToolContext` 可正常引用
- `use ghostcode_mcp::ToolError` 可正常引用（通过 re-export）

---

### Task 12: 编写 TDD 测试（8 个工具 + 公共逻辑）

**文件**: `crates/ghostcode-mcp/src/tools/mod.rs`（追加测试模块）
**依赖**: Task 1-11 全部完成
**可并行**: 否

#### 测试策略

TDD 测试不连接真实 Daemon，只验证：
1. 参数解析和验证逻辑（缺失必填参数 → 正确错误类型）
2. DaemonRequest 构造正确性（op 名称、args 结构）
3. schema() 返回格式正确（包含 name / description / inputSchema）

通过 Mock 或直接验证构造的 DaemonRequest 来绕过 Daemon 连接。

#### 完整测试实现

在 `crates/ghostcode-mcp/src/tools/mod.rs` 末尾追加：

```rust
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
    fn all_schemas_returns_8_tools() {
        let schemas = all_tool_schemas();
        assert_eq!(schemas.len(), 8, "必须恰好返回 8 个工具定义");

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
    // 测试 9: message_send to 字段 - 字符串转 Vec 正确
    // --------------------------------------------------------
    // 此测试通过检查 DaemonRequest 构造过程间接验证
    // （message_send 内部逻辑，Daemon 未启动时会返回连接错误）
    // 注意：此测试预期 Err(ToolError::Daemon)，确认参数解析通过后才报连接错误
    #[tokio::test]
    async fn message_send_with_text_reaches_daemon_stage() {
        let ctx = mock_ctx();
        let result = super::message_send::execute(
            &serde_json::json!({"text": "hello", "to": "peer-1"}),
            &ctx,
        ).await;
        // 参数解析通过，到达 Daemon 调用阶段，因为 socket 不存在所以返回 Daemon 错误
        // 而非 MissingParam 或 InvalidParam
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
```

#### 验收标准
- `cargo test -p ghostcode-mcp` 全部 10 个测试通过
- 测试 3、4、5、6、7 不需要 Daemon 连接（纯参数验证）
- 测试 9 预期 `Daemon` 错误（socket 不存在），不是参数错误

---

### Task 13: 编译验证

**操作**: 运行编译和测试命令，确认无错误
**依赖**: Task 1-12 全部完成
**可并行**: 否

执行顺序：

```bash
# 步骤 1: 编译验证
cargo build -p ghostcode-mcp 2>&1

# 步骤 2: 测试验证（包含 T14 原有测试 + T15 新增测试）
cargo test -p ghostcode-mcp 2>&1
```

预期结果：
- 步骤 1：无 error，允许有 warning（如 dead_code）
- 步骤 2：全部测试通过。预计测试数量：T14 原有 5 个 + proptest 1 个 + T15 新增 10 个 = 16 个

如果编译失败，优先检查：
1. `pub(crate) fn call_daemon` 是否已正确修改可见性
2. `crate::tools::` 路径引用是否正确
3. tools 子模块的 use 路径是否正确（`use crate::server::call_daemon`）

---

## 文件冲突检查

| 文件 | 状态 | 说明 |
|------|------|------|
| `crates/ghostcode-mcp/src/tools/mod.rs` | **新建** | Task 1 创建 |
| `crates/ghostcode-mcp/src/tools/message_send.rs` | **新建** | Task 2 创建 |
| `crates/ghostcode-mcp/src/tools/inbox_list.rs` | **新建** | Task 3 创建 |
| `crates/ghostcode-mcp/src/tools/inbox_mark_read.rs` | **新建** | Task 4 创建 |
| `crates/ghostcode-mcp/src/tools/inbox_mark_all_read.rs` | **新建** | Task 5 创建 |
| `crates/ghostcode-mcp/src/tools/actor_list.rs` | **新建** | Task 6 创建 |
| `crates/ghostcode-mcp/src/tools/actor_start.rs` | **新建** | Task 7 创建 |
| `crates/ghostcode-mcp/src/tools/actor_stop.rs` | **新建** | Task 8 创建 |
| `crates/ghostcode-mcp/src/tools/group_info.rs` | **新建** | Task 9 创建 |
| `crates/ghostcode-mcp/src/server.rs` | **修改** | Task 10：4 处修改（pub(crate) call_daemon + 替换 static_tool_list + 更新 tools/list + 更新 tools/call） |
| `crates/ghostcode-mcp/src/lib.rs` | **修改** | Task 11：添加 pub mod tools 声明 |
| `crates/ghostcode-mcp/Cargo.toml` | **无需修改** | thiserror 已在工作区依赖中 |

---

## TDD 强制执行规范

本任务必须严格遵循 TDD 流程：Red → Green → Refactor。

```
Red    → 先写测试代码（Task 12）+ 创建最小 stub（仅签名，body 用 todo!()），测试编译但失败
Green  → 写完整实现（Task 1-9），让所有测试通过
Refactor → 集成（Task 10-11）+ 编译验证（Task 13）
```

---

## 并行分组

```
阶段 A（TDD Red — 测试先行）:
  Task 12: 编写测试 + 创建最小 stub（tools/mod.rs 仅签名 + 8 个工具文件仅签名）
  Task 11: 更新 lib.rs（导出 tools 模块，让测试能编译）
  验证: cargo test -p ghostcode-mcp 编译通过但测试失败（Red）

阶段 B（TDD Green — 实现让测试通过，可全部并行）:
  Task 1:  tools/mod.rs 完整实现（ToolContext / ToolError / dispatch_tool）
  Task 2:  tools/message_send.rs 完整实现
  Task 3:  tools/inbox_list.rs 完整实现
  Task 4:  tools/inbox_mark_read.rs 完整实现
  Task 5:  tools/inbox_mark_all_read.rs 完整实现
  Task 6:  tools/actor_list.rs 完整实现
  Task 7:  tools/actor_start.rs 完整实现
  Task 8:  tools/actor_stop.rs 完整实现
  Task 9:  tools/group_info.rs 完整实现

阶段 C（TDD Refactor — 集成 + 验证）:
  Task 10: 更新 server.rs（集成工具分发）
  Task 13: 编译验证 + cargo test 全部通过（Green 确认）
```

时序图：

```
Task 12 + Task 11 (Red: 测试 + stub) ──────────────────────┐
                                                             v
Task 1 ─┐
Task 2 ─┤
Task 3 ─┤
Task 4 ─┤
Task 5 ─┼──> Task 10 ──> Task 13 (Refactor: 集成 + 验证)
Task 6 ─┤    (Green: 完整实现)
Task 7 ─┤
Task 8 ─┤
Task 9 ─┘
```

---

## Builder 配置

Builder 执行本计划时，必须按以下 TDD 顺序操作：

### 阶段 A — Red（测试先行）

1. 创建 `crates/ghostcode-mcp/src/tools/` 目录
2. 创建 stub 文件：`tools/mod.rs`（仅 ToolContext/ToolError 类型定义 + dispatch_tool/all_tool_schemas 签名，body 用 `todo!()`）
3. 创建 8 个工具 stub 文件（仅 `pub async fn execute()` 签名，body 用 `todo!()`）
4. 覆盖 `crates/ghostcode-mcp/src/lib.rs`，写入 Task 11 内容（导出 tools 模块）
5. 在 `tools/mod.rs` 末尾追加 Task 12 的完整测试模块
6. 运行 `cargo test -p ghostcode-mcp` 确认测试**编译通过但断言失败**（Red 状态）

### 阶段 B — Green（实现让测试通过）

7. 补全 `tools/mod.rs` 的完整实现（Task 1，替换 `todo!()` 为真实逻辑）
8. 补全 8 个工具文件的完整实现（Task 2-9）
9. 运行 `cargo test -p ghostcode-mcp` 确认所有测试**通过**（Green 状态）

### 阶段 C — Refactor（集成 + 验证）

10. 修改 `crates/ghostcode-mcp/src/server.rs`（Task 10）：
    - 将 `async fn call_daemon` 改为 `pub(crate) async fn call_daemon`
    - 删除 `fn static_tool_list()` 整个函数
    - 将 `tools/list` 处理分支中的 `static_tool_list()` 替换为 `crate::tools::all_tool_schemas()`
    - 将 `tools/call` 处理分支替换为 Task 10 中的新实现
11. 运行 `cargo build -p ghostcode-mcp` 验证零警告
12. 运行 `cargo test -p ghostcode-mcp` 最终确认全部通过

**注意事项**：
- 所有文件头注释日期为 2026-03-01
- 作者署名统一为 `Atlas.oi`
- 禁止使用 emoji
- `call_daemon` 对 Daemon 连接失败必须返回 `Err`，禁止降级为空结果
- 工具描述（description 字段）使用英文（MCP 标准要求）
- 注释和文档使用中文
