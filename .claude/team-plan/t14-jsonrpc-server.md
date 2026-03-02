# Team Plan: T14 stdio JSON-RPC 2.0 Server

## 概述

为 `ghostcode-mcp` crate 实现 stdio 模式的 JSON-RPC 2.0 服务器框架。
该框架是 MCP Plugin 的核心入口，从 stdin 读取请求、处理 MCP 标准方法（initialize / tools/list / tools/call），并向 stdout 写出响应。
内部通过 Unix Socket 连接 Daemon 转发工具调用请求。

**产出文件**:
- `crates/ghostcode-mcp/src/jsonrpc.rs` — JSON-RPC 2.0 数据结构 + 基础构造函数
- `crates/ghostcode-mcp/src/server.rs` — stdio 主循环 + 方法路由 + Daemon IPC 调用

**前置依赖**: T02（ghostcode-types ipc.rs 中的 DaemonRequest / DaemonResponse，已完成）

---

## Codex 分析摘要

Codex CLI 不可用，由 Claude 自行分析。

---

## Gemini 分析摘要

批量计划生成模式，跳过多模型分析。

---

## 技术方案

### 参考实现溯源

- 参考: `cccc/src/cccc/ports/mcp/main.py:76-103` — `_read_message` / `_write_message` / `_make_response` / `_make_error`
- 参考: `cccc/src/cccc/ports/mcp/main.py:106-225` — `handle_request` 方法路由逻辑
- 参考: `cccc/src/cccc/ports/mcp/main.py:228-241` — `main()` stdio 主循环
- 参考: `cccc/src/cccc/ports/mcp/common.py:125-147` — `_call_daemon_or_raise` Unix Socket IPC

### 架构设计

```
stdin (逐行)
    |
    v
read_line() -> serde_json::from_str -> JsonRpcRequest
    |
    v
dispatch(method)
    ├── "initialize"       -> handle_initialize()
    ├── "tools/list"       -> handle_tools_list()
    ├── "tools/call"       -> handle_tools_call() -> call_daemon(Unix Socket)
    ├── "notifications/*"  -> None (无响应)
    ├── "resources/list"   -> make_response({"resources":[]})
    ├── "prompts/list"     -> make_response({"prompts":[]})
    ├── "ping"             -> make_response({})
    ├── "logging/setLevel" -> make_response({})
    └── unknown            -> make_error(-32601, "Method not found")
                                         |
                                         v
                              serde_json::to_string -> stdout + "\n" + flush
```

### 关键设计决策

1. **逐行 stdin 读取**: 使用 `tokio::io::BufReader` + `lines()` 异步读取，EOF 时退出主循环（对应 cccc main.py:230-232）
2. **JSON-RPC id 类型**: 使用 `serde_json::Value`，支持 Number / String / Null 三种 MCP 合规 id 类型
3. **Notifications 无响应**: method 以 `notifications/` 开头时返回 `None`，主循环跳过写出（对应 cccc main.py:131-133）
4. **Daemon IPC**: 通过 `tokio::net::UnixStream` 连接 daemon_addr，发送 `DaemonRequest` JSON + `\n`，读取 `DaemonResponse` JSON
5. **工具列表**: 当前 T14 阶段返回 8 个静态占位工具（后续 T15 补全），满足测试 `tools_list_returns_8`
6. **环境变量身份注入**: 从 `GHOSTCODE_GROUP_ID` / `GHOSTCODE_ACTOR_ID` 读取，不从参数读取

---

## 子任务列表

---

### Task 1: 创建 jsonrpc.rs — JSON-RPC 2.0 数据结构

**文件**: `crates/ghostcode-mcp/src/jsonrpc.rs`
**依赖**: 无
**可并行**: 是（与 Task 2 并行）

#### 完整实现内容

```rust
//! JSON-RPC 2.0 数据结构定义
//!
//! 定义 MCP stdio 传输层使用的请求/响应/错误类型
//! 参考: cccc/src/cccc/ports/mcp/main.py:93-103 - _make_response / _make_error
//!
//! @author Atlas.oi
//! @date 2026-03-01

use serde::{Deserialize, Serialize};

/// JSON-RPC 2.0 请求
///
/// 从 stdin 逐行读取并反序列化
/// id 为 serde_json::Value，支持 Number / String / Null
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    /// 协议版本，固定为 "2.0"
    pub jsonrpc: String,
    /// 请求 ID，Number / String / Null
    pub id: serde_json::Value,
    /// 方法名
    pub method: String,
    /// 方法参数，可为 object 或 array
    #[serde(default)]
    pub params: serde_json::Value,
}

/// JSON-RPC 2.0 响应
///
/// 序列化后写入 stdout，末尾加 "\n" 并 flush
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    /// 协议版本，固定为 "2.0"
    pub jsonrpc: String,
    /// 与请求对应的 ID
    pub id: serde_json::Value,
    /// 成功时的结果（与 error 互斥）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// 失败时的错误对象（与 result 互斥）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 错误对象
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcError {
    /// 错误码（参见 JSON-RPC 2.0 规范）
    /// -32601: Method not found
    /// -32600: Invalid Request
    /// -32603: Internal error
    pub code: i32,
    /// 可读错误描述
    pub message: String,
    /// 附加错误数据（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcResponse {
    /// 构造成功响应
    ///
    /// # 参数
    /// - id: 对应请求的 ID，原样返回
    /// - result: 任意 JSON 值
    pub fn ok(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// 构造错误响应
    ///
    /// # 参数
    /// - id: 对应请求的 ID，原样返回
    /// - code: 错误码（如 -32601）
    /// - message: 可读错误描述
    pub fn error(id: serde_json::Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}
```

#### 验收标准
- `JsonRpcRequest` 可反序列化 `{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}`
- `JsonRpcResponse::ok(id, result)` 序列化后包含 `"jsonrpc":"2.0"` 和对应 `"id"`
- `JsonRpcResponse::error(id, -32601, "...")` 序列化后不含 `"result"` 字段

---

### Task 2: 创建 server.rs — 会话状态结构体与 Daemon IPC 函数

**文件**: `crates/ghostcode-mcp/src/server.rs`（第一部分）
**依赖**: Task 1 完成（jsonrpc.rs 存在）
**可并行**: Task 1 完成后立即开始，可与 Task 3 基础代码同时进行

#### 完整实现内容（server.rs 上半部分）

```rust
//! GhostCode MCP stdio JSON-RPC 2.0 服务器
//!
//! 实现 MCP 标准协议的 stdio 服务器主循环
//! 从 stdin 逐行读取 JSON-RPC 请求，处理后写入 stdout
//! 内部通过 Unix Socket 连接 Daemon 转发工具调用
//!
//! 参考: cccc/src/cccc/ports/mcp/main.py:228-241 - stdio 主循环
//! 参考: cccc/src/cccc/ports/mcp/common.py:125-147 - Daemon IPC 调用
//!
//! @author Atlas.oi
//! @date 2026-03-01

use std::path::Path;
use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use ghostcode_types::ipc::{DaemonRequest, DaemonResponse};

use crate::jsonrpc::{JsonRpcRequest, JsonRpcResponse};

// ============================================================
// 会话状态（单会话生命周期）
// 存储 initialize 握手时协商的客户端能力
// ============================================================

/// MCP 客户端能力（从 initialize params 中解析）
///
/// 参考: cccc/src/cccc/ports/mcp/main.py:26-35
#[derive(Debug, Default)]
struct SessionState {
    /// 客户端是否支持 tools/list_changed 通知
    supports_tools_list_changed: bool,
}

impl SessionState {
    /// 从 initialize params 解析客户端能力
    ///
    /// params 结构:
    /// {
    ///   "capabilities": {
    ///     "tools": { "listChanged": true }
    ///   }
    /// }
    fn from_init_params(params: &serde_json::Value) -> Self {
        let supports = params
            .get("capabilities")
            .and_then(|c| c.get("tools"))
            .and_then(|t| t.get("listChanged"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        Self { supports_tools_list_changed: supports }
    }
}

// ============================================================
// Daemon IPC — Unix Socket 调用
// 参考: cccc/src/cccc/ports/mcp/common.py:125-147
// ============================================================

/// 调用 Daemon 并返回结果
///
/// 业务逻辑：
/// 1. 连接到 daemon_addr 指定的 Unix Socket
/// 2. 将 DaemonRequest 序列化为 JSON 并写入，末尾加 "\n"
/// 3. 读取一行响应并反序列化为 DaemonResponse
/// 4. 若 ok=false，返回 Err 包含错误信息
///
/// # 参数
/// - daemon_addr: Daemon Unix Socket 路径
/// - req: DaemonRequest 对象
///
/// # 返回
/// - Ok(serde_json::Value): Daemon 返回的 result 字段
/// - Err(anyhow::Error): 连接失败、协议错误或 Daemon 返回 ok=false
async fn call_daemon(
    daemon_addr: &Path,
    req: DaemonRequest,
) -> Result<serde_json::Value> {
    use tokio::net::UnixStream;

    // 连接 Daemon Unix Socket
    let stream = UnixStream::connect(daemon_addr).await?;
    let (reader, mut writer) = tokio::io::split(stream);

    // 序列化请求并写入（末尾加换行）
    let req_json = serde_json::to_string(&req)?;
    writer.write_all(req_json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    // 读取一行响应
    let mut buf_reader = BufReader::new(reader);
    let mut line = String::new();
    buf_reader.read_line(&mut line).await?;

    // 反序列化响应
    let resp: DaemonResponse = serde_json::from_str(line.trim())?;

    if resp.ok {
        Ok(resp.result)
    } else {
        let err = resp.error.as_ref();
        let msg = err
            .map(|e| format!("[{}] {}", e.code, e.message))
            .unwrap_or_else(|| "daemon error".to_string());
        Err(anyhow::anyhow!(msg))
    }
}
```

#### 验收标准
- `SessionState::from_init_params` 对 `{"capabilities":{"tools":{"listChanged":true}}}` 返回 `supports_tools_list_changed = true`
- `call_daemon` 函数签名正确，编译通过

---

### Task 3: 实现 handle_request — 方法路由

**文件**: `crates/ghostcode-mcp/src/server.rs`（第二部分，追加）
**依赖**: Task 2 完成
**可并行**: 否

#### 完整实现内容

```rust
// ============================================================
// 静态工具列表（T14 阶段，8 个占位工具）
// 后续 T15 任务补全为完整 MCP 工具定义
// ============================================================

/// 返回静态工具定义列表（当前 8 个占位）
///
/// 格式遵循 MCP tools/list 规范：
/// { "name": "...", "description": "...", "inputSchema": {...} }
///
/// 参考: cccc/src/cccc/ports/mcp/toolspecs.py - MCP_TOOLS 结构
fn static_tool_list() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "ghostcode_help",
            "description": "获取 GhostCode 帮助信息",
            "inputSchema": {"type": "object", "properties": {}, "required": []}
        },
        {
            "name": "ghostcode_bootstrap",
            "description": "会话启动引导",
            "inputSchema": {"type": "object", "properties": {}, "required": []}
        },
        {
            "name": "ghostcode_inbox_list",
            "description": "列出未读消息",
            "inputSchema": {"type": "object", "properties": {}, "required": []}
        },
        {
            "name": "ghostcode_inbox_mark_read",
            "description": "标记消息已读",
            "inputSchema": {"type": "object", "properties": {"event_id": {"type": "string"}}, "required": ["event_id"]}
        },
        {
            "name": "ghostcode_message_send",
            "description": "发送消息给 Agent",
            "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}
        },
        {
            "name": "ghostcode_message_reply",
            "description": "回复消息",
            "inputSchema": {"type": "object", "properties": {"event_id": {"type": "string"}, "text": {"type": "string"}}, "required": ["event_id", "text"]}
        },
        {
            "name": "ghostcode_actor_list",
            "description": "列出所有 Actor",
            "inputSchema": {"type": "object", "properties": {}, "required": []}
        },
        {
            "name": "ghostcode_group_info",
            "description": "获取 Group 信息",
            "inputSchema": {"type": "object", "properties": {}, "required": []}
        }
    ])
}

// ============================================================
// 核心方法路由
// 参考: cccc/src/cccc/ports/mcp/main.py:106-225
// ============================================================

/// 处理单条 JSON-RPC 请求，返回响应（Notification 返回 None）
///
/// 方法路由规则：
/// - "initialize"        → 返回 serverInfo + capabilities
/// - "notifications/*"   → 返回 None（无需响应）
/// - "tools/list"        → 返回静态工具列表（分页支持）
/// - "tools/call"        → 转发至 Daemon
/// - "resources/list"    → 返回 {"resources":[]}
/// - "prompts/list"      → 返回 {"prompts":[]}
/// - "ping"              → 返回 {}
/// - "logging/setLevel"  → 返回 {}
/// - 其他                → error -32601 Method not found
///
/// # 参数
/// - req: 已解析的 JSON-RPC 请求
/// - state: 当前会话状态（initialize 时更新）
/// - daemon_addr: Daemon Unix Socket 路径
///
/// # 返回
/// - Some(JsonRpcResponse): 需要写回 stdout 的响应
/// - None: Notification，不写响应
async fn handle_request(
    req: JsonRpcRequest,
    state: &mut SessionState,
    daemon_addr: &Path,
) -> Option<JsonRpcResponse> {
    let id = req.id.clone();
    let method = req.method.as_str();
    let params = &req.params;

    match method {
        // --------------------------------------------------
        // initialize: 协商能力，返回 serverInfo
        // 参考: cccc/src/cccc/ports/mcp/main.py:113-129
        // --------------------------------------------------
        "initialize" => {
            // 解析并存储客户端能力
            *state = SessionState::from_init_params(params);

            let result = serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": { "listChanged": true },
                    "resources": {},
                    "prompts": {}
                },
                "serverInfo": {
                    "name": "ghostcode-mcp",
                    "version": env!("CARGO_PKG_VERSION")
                }
            });
            Some(JsonRpcResponse::ok(id, result))
        }

        // --------------------------------------------------
        // notifications/*: 客户端通知，不需要响应
        // 参考: cccc/src/cccc/ports/mcp/main.py:131-133
        // --------------------------------------------------
        m if m.starts_with("notifications/") => None,

        // --------------------------------------------------
        // tools/list: 返回工具列表（支持简单分页）
        // 参考: cccc/src/cccc/ports/mcp/main.py:135-147
        // --------------------------------------------------
        "tools/list" => {
            let tools = static_tool_list();
            let tools_arr = tools.as_array().cloned().unwrap_or_default();
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

            // 计算 nextCursor（若有更多页）
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

        // --------------------------------------------------
        // tools/call: 工具调用，转发至 Daemon
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

            // 从环境变量注入身份信息
            let group_id = std::env::var("GHOSTCODE_GROUP_ID").unwrap_or_default();
            let actor_id = std::env::var("GHOSTCODE_ACTOR_ID").unwrap_or_default();

            // 构造 Daemon 请求
            let daemon_req = DaemonRequest::new(
                "tool_call",
                serde_json::json!({
                    "group_id": group_id,
                    "actor_id": actor_id,
                    "tool_name": tool_name,
                    "arguments": arguments
                }),
            );

            // 转发至 Daemon
            match call_daemon(daemon_addr, daemon_req).await {
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
                    // 工具调用失败：MCP 规范要求以 isError:true 返回
                    let content = serde_json::json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::json!({
                                "error": {
                                    "code": "tool_call_failed",
                                    "message": e.to_string()
                                }
                            }).to_string()
                        }],
                        "isError": true
                    });
                    Some(JsonRpcResponse::ok(id, content))
                }
            }
        }

        // --------------------------------------------------
        // 以下为可选 MCP 接口，返回空列表避免客户端报错
        // 参考: cccc/src/cccc/ports/mcp/main.py:149-161
        // --------------------------------------------------
        "resources/list" => {
            Some(JsonRpcResponse::ok(id, serde_json::json!({"resources": []})))
        }

        "prompts/list" => {
            Some(JsonRpcResponse::ok(id, serde_json::json!({"prompts": []})))
        }

        "ping" => {
            Some(JsonRpcResponse::ok(id, serde_json::json!({})))
        }

        "logging/setLevel" => {
            Some(JsonRpcResponse::ok(id, serde_json::json!({})))
        }

        // --------------------------------------------------
        // 未知方法：返回标准错误码 -32601
        // 参考: cccc/src/cccc/ports/mcp/main.py:224-225
        // --------------------------------------------------
        _ => {
            Some(JsonRpcResponse::error(
                id,
                -32601,
                format!("Method not found: {}", method),
            ))
        }
    }
}
```

#### 验收标准
- `handle_request` 对 `initialize` 返回包含 `protocolVersion` 的响应
- `handle_request` 对 `notifications/initialized` 返回 `None`
- `handle_request` 对 `foo/bar` 返回 error code `-32601`
- `handle_request` 对 `tools/list` 返回含 8 个工具的列表

---

### Task 4: 实现 serve_stdio — 主循环 + pub API

**文件**: `crates/ghostcode-mcp/src/server.rs`（第三部分，追加）
**依赖**: Task 3 完成
**可并行**: 否

#### 完整实现内容

```rust
// ============================================================
// 公共入口函数
// 参考: cccc/src/cccc/ports/mcp/main.py:228-241 - main()
// ============================================================

/// stdio JSON-RPC 2.0 服务器主循环
///
/// 业务逻辑：
/// 1. 初始化空会话状态
/// 2. 逐行读取 stdin，EOF 时退出
/// 3. 每行反序列化为 JsonRpcRequest（解析失败则返回 -32700 错误）
/// 4. 调用 handle_request 分派方法
/// 5. 若有响应（非 Notification），序列化写入 stdout + "\n" + flush
///
/// # 参数
/// - group_id: GhostCode 工作组 ID（目前通过环境变量传递，参数备用）
/// - actor_id: Actor ID（目前通过环境变量传递，参数备用）
/// - daemon_addr: Daemon Unix Socket 路径
///
/// # 返回
/// - Ok(()): stdin EOF 正常退出
/// - Err(anyhow::Error): 致命 IO 错误
pub async fn serve_stdio(
    _group_id: &str,
    _actor_id: &str,
    daemon_addr: &Path,
) -> Result<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let mut reader = BufReader::new(stdin).lines();
    let mut writer = tokio::io::BufWriter::new(stdout);

    // 会话状态（单实例，随 initialize 握手更新）
    let mut state = SessionState::default();

    // 主循环：逐行读取直到 EOF
    while let Some(line) = reader.next_line().await? {
        let line = line.trim().to_string();

        // 跳过空行
        if line.is_empty() {
            continue;
        }

        // 解析 JSON-RPC 请求
        let resp = match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(req) => {
                // 分派到方法处理器
                handle_request(req, &mut state, daemon_addr).await
            }
            Err(_) => {
                // 解析失败：返回 -32700 Parse error（id 无法确定，用 Null）
                Some(JsonRpcResponse::error(
                    serde_json::Value::Null,
                    -32700,
                    "Parse error",
                ))
            }
        };

        // 写出响应（Notification 跳过）
        if let Some(resp) = resp {
            let resp_json = serde_json::to_string(&resp)?;
            writer.write_all(resp_json.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
        }
    }

    Ok(())
}
```

#### 验收标准
- 函数签名与规格要求完全一致：`async fn serve_stdio(group_id: &str, actor_id: &str, daemon_addr: &Path) -> Result<()>`
- EOF 时函数返回 `Ok(())`
- 空行被跳过
- JSON 解析失败时输出 -32700 错误

---

### Task 5: 更新 lib.rs — 导出 server 和 jsonrpc 模块

**文件**: `crates/ghostcode-mcp/src/lib.rs`
**依赖**: Task 1、Task 4 完成
**可并行**: 否

#### 完整实现内容

将现有 `lib.rs` 内容替换为：

```rust
//! GhostCode MCP Server
//!
//! 将 GhostCode 功能暴露为 MCP 工具供 Claude Code 调用
//! 实现 stdio JSON-RPC 2.0 服务器协议
//!
//! @author Atlas.oi
//! @date 2026-03-01

pub mod jsonrpc;
pub mod server;

// 重新导出公共 API
pub use server::serve_stdio;
```

#### 验收标准
- `use ghostcode_mcp::serve_stdio` 可正常引用
- `use ghostcode_mcp::jsonrpc::{JsonRpcRequest, JsonRpcResponse}` 可正常引用

---

### Task 6: 编写单元测试

**文件**: `crates/ghostcode-mcp/src/server.rs`（测试模块追加）和 `crates/ghostcode-mcp/src/jsonrpc.rs`（测试模块追加）
**依赖**: Task 4 完成
**可并行**: 否

#### 完整测试实现

在 `server.rs` 末尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonrpc::{JsonRpcRequest, JsonRpcResponse};

    // --------------------------------------------------------
    // 测试辅助：构造 JsonRpcRequest
    // --------------------------------------------------------
    fn make_req(method: &str, id: serde_json::Value, params: serde_json::Value) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        }
    }

    // --------------------------------------------------------
    // 测试 1: initialize_handshake
    // 发送 initialize 请求，响应必须包含 protocolVersion
    // --------------------------------------------------------
    #[tokio::test]
    async fn initialize_handshake() {
        let req = make_req("initialize", serde_json::json!(1), serde_json::json!({
            "capabilities": { "tools": { "listChanged": true } }
        }));
        let mut state = SessionState::default();
        // 使用不存在的路径（initialize 不访问 daemon）
        let daemon_addr = std::path::Path::new("/nonexistent/daemon.sock");
        let resp = handle_request(req, &mut state, daemon_addr).await;

        let resp = resp.expect("initialize 必须返回响应");
        assert_eq!(resp.jsonrpc, "2.0");

        let result = resp.result.expect("initialize 必须有 result");
        assert!(result.get("protocolVersion").is_some(), "响应必须包含 protocolVersion");
        assert!(result.get("capabilities").is_some(), "响应必须包含 capabilities");
        assert!(result.get("serverInfo").is_some(), "响应必须包含 serverInfo");

        // 验证会话状态已更新
        assert!(state.supports_tools_list_changed, "initialize 后应记录客户端能力");
    }

    // --------------------------------------------------------
    // 测试 2: tools_list_returns_8
    // tools/list 必须返回恰好 8 个工具定义
    // --------------------------------------------------------
    #[tokio::test]
    async fn tools_list_returns_8() {
        let req = make_req("tools/list", serde_json::json!(2), serde_json::json!({}));
        let mut state = SessionState::default();
        let daemon_addr = std::path::Path::new("/nonexistent/daemon.sock");
        let resp = handle_request(req, &mut state, daemon_addr).await;

        let resp = resp.expect("tools/list 必须返回响应");
        let result = resp.result.expect("tools/list 必须有 result");
        let tools = result["tools"].as_array().expect("tools 必须是数组");
        assert_eq!(tools.len(), 8, "工具数量必须恰好为 8");
    }

    // --------------------------------------------------------
    // 测试 3: response_always_has_jsonrpc_and_id
    // 所有响应必须包含 jsonrpc:"2.0" 和与请求对应的 id
    // --------------------------------------------------------
    #[tokio::test]
    async fn response_always_has_jsonrpc_and_id() {
        let methods = [
            ("initialize", serde_json::json!({})),
            ("tools/list", serde_json::json!({})),
            ("resources/list", serde_json::json!({})),
            ("prompts/list", serde_json::json!({})),
            ("ping", serde_json::json!({})),
            ("logging/setLevel", serde_json::json!({})),
        ];

        for (method, params) in &methods {
            let req_id = serde_json::json!(42);
            let req = make_req(method, req_id.clone(), params.clone());
            let mut state = SessionState::default();
            let daemon_addr = std::path::Path::new("/nonexistent/daemon.sock");
            let resp = handle_request(req, &mut state, daemon_addr).await;

            let resp = resp.expect(&format!("{} 必须返回响应", method));
            assert_eq!(resp.jsonrpc, "2.0", "{}: jsonrpc 字段必须为 '2.0'", method);
            assert_eq!(resp.id, req_id, "{}: id 必须与请求一致", method);
        }
    }

    // --------------------------------------------------------
    // 测试 4: unknown_method_error
    // 未知方法必须返回 error code -32601
    // --------------------------------------------------------
    #[tokio::test]
    async fn unknown_method_error() {
        let req = make_req("foo/bar", serde_json::json!(99), serde_json::json!({}));
        let mut state = SessionState::default();
        let daemon_addr = std::path::Path::new("/nonexistent/daemon.sock");
        let resp = handle_request(req, &mut state, daemon_addr).await;

        let resp = resp.expect("未知方法也必须返回响应");
        assert_eq!(resp.jsonrpc, "2.0");
        assert_eq!(resp.id, serde_json::json!(99));
        assert!(resp.result.is_none(), "错误响应不应含 result");

        let error = resp.error.expect("未知方法必须返回 error");
        assert_eq!(error.code, -32601, "错误码必须为 -32601");
    }

    // --------------------------------------------------------
    // 测试 5: notifications 不返回响应
    // --------------------------------------------------------
    #[tokio::test]
    async fn notification_returns_none() {
        let req = make_req(
            "notifications/initialized",
            serde_json::Value::Null,
            serde_json::json!({}),
        );
        let mut state = SessionState::default();
        let daemon_addr = std::path::Path::new("/nonexistent/daemon.sock");
        let resp = handle_request(req, &mut state, daemon_addr).await;
        assert!(resp.is_none(), "Notification 不应返回响应");
    }
}
```

在 `jsonrpc.rs` 末尾追加 proptest：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // proptest: response_id_matches_request
    // 任意 i64 id，JsonRpcResponse::ok(id, result).id 必须与输入 id 一致
    proptest! {
        #[test]
        fn response_id_matches_request(n in any::<i64>()) {
            let id = serde_json::json!(n);
            let resp = JsonRpcResponse::ok(id.clone(), serde_json::json!({}));
            prop_assert_eq!(resp.id, id, "响应 id 必须与请求 id 一致");
            prop_assert_eq!(resp.jsonrpc, "2.0", "jsonrpc 字段必须为 '2.0'");
        }
    }

    #[test]
    fn ok_response_has_no_error_field() {
        let resp = JsonRpcResponse::ok(serde_json::json!(1), serde_json::json!({}));
        assert!(resp.error.is_none(), "成功响应不应含 error 字段");

        // 验证序列化后不含 error 键
        let json_str = serde_json::to_string(&resp).unwrap();
        assert!(!json_str.contains("\"error\""), "序列化后不应含 error 字段");
    }

    #[test]
    fn error_response_has_no_result_field() {
        let resp = JsonRpcResponse::error(serde_json::json!(1), -32601, "Method not found");
        assert!(resp.result.is_none(), "错误响应不应含 result 字段");

        // 验证序列化后不含 result 键
        let json_str = serde_json::to_string(&resp).unwrap();
        assert!(!json_str.contains("\"result\""), "序列化后不应含 result 字段");
    }
}
```

#### 验收标准
- `cargo test -p ghostcode-mcp` 全部测试通过（包含 5 个普通测试 + 1 个 proptest）
- proptest 100 次随机用例全部通过

---

### Task 7: 编译验证

**操作**: 运行 `cargo build -p ghostcode-mcp` 确认无编译错误
**依赖**: Task 5、Task 6 完成
**可并行**: 否

执行命令：
```bash
cargo build -p ghostcode-mcp 2>&1
```

预期：输出无 error，最多有 warning（允许 dead_code 警告，因 serve_stdio 的 _group_id / _actor_id 参数在当前版本未使用）。

执行命令：
```bash
cargo test -p ghostcode-mcp 2>&1
```

预期：所有测试通过，包括 proptest。

---

## 文件冲突检查

| 文件 | 状态 | 说明 |
|------|------|------|
| `crates/ghostcode-mcp/src/lib.rs` | **已存在（覆盖）** | 当前只有文件头注释，Task 5 覆盖添加模块声明 |
| `crates/ghostcode-mcp/src/jsonrpc.rs` | **新建** | 不存在，Task 1 创建 |
| `crates/ghostcode-mcp/src/server.rs` | **新建** | 不存在，Task 2-4 创建 |
| `crates/ghostcode-mcp/Cargo.toml` | **无需修改** | 依赖已含 serde / serde_json / tokio / anyhow / proptest |

---

## TDD 强制执行规范

本任务必须严格遵循 TDD 流程：Red → Green → Refactor。

```
Red    → 先写测试代码（Task 6），创建最小 stub 让测试编译但断言失败
Green  → 写实现代码（Task 1-5），让所有测试通过
Refactor → 集成 + 编译验证（Task 7）
```

---

## 并行分组

```
阶段 A（TDD Red — 测试先行）:
  Task 6: 编写测试 + 创建最小类型 stub（仅签名，函数体 todo!/unimplemented!）
  验证: cargo test -p ghostcode-mcp 编译通过但测试失败（Red）

阶段 B（TDD Green — 实现让测试通过）:
  Task 1: 创建 jsonrpc.rs（完整实现）
  Task 2: server.rs 会话状态 + Daemon IPC
  -> Task 3: handle_request 方法路由（串行追加）
  -> Task 4: serve_stdio 主循环（串行追加）
  -> Task 5: 更新 lib.rs

阶段 C（TDD Refactor — 验证）:
  Task 7: 编译验证 + cargo test 全部通过（Green 确认）
```

时序图：

```
Task 6 (Red: 测试先写) ─────────────────────────┐
                                                  v
Task 1 → Task 2 → Task 3 → Task 4 → Task 5 (Green: 实现)
                                                  \
                                              Task 7 (Refactor: 验证)
```

---

## Builder 配置

Builder 执行本计划时，必须按以下 TDD 顺序操作：

**阶段 A — Red（测试先行）**:
1. 创建最小 stub 文件：`jsonrpc.rs`（仅类型签名）、`server.rs`（仅函数签名，body 用 `todo!()`）、`lib.rs`（模块声明）
2. 写入 Task 6 的完整测试代码（`server.rs` 测试模块 + `jsonrpc.rs` proptest 模块）
3. 运行 `cargo test -p ghostcode-mcp` 确认测试**编译通过但断言失败**（Red 状态）

**阶段 B — Green（实现让测试通过）**:
4. 补全 `jsonrpc.rs` 的完整实现（Task 1）
5. 补全 `server.rs` 的完整实现（Task 2 → 3 → 4）
6. 更新 `lib.rs`（Task 5）
7. 运行 `cargo test -p ghostcode-mcp` 确认所有测试**通过**（Green 状态）

**阶段 C — Refactor（验证）**:
8. 运行 `cargo build -p ghostcode-mcp` 验证零警告
9. 运行 `cargo test -p ghostcode-mcp` 最终确认

**注意事项**:
- 所有代码文件头必须包含中文注释，日期为 2026-03-01
- 作者署名统一为 `Atlas.oi`
- 禁止使用 emoji
- 禁止使用降级回退策略（如 daemon 连接失败不得静默返回空结果）
