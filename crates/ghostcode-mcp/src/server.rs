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
/// 注: supports_tools_list_changed 在 T15 实现 tools/list_changed 通知时使用
#[derive(Debug, Default)]
#[allow(dead_code)]
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
// Daemon IPC -- Unix Socket 调用
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
pub(crate) async fn call_daemon(
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

// ============================================================
// 核心方法路由
// 参考: cccc/src/cccc/ports/mcp/main.py:106-225
// ============================================================

/// 处理单条 JSON-RPC 请求，返回响应（Notification 返回 None）
///
/// 方法路由规则：
/// - "initialize"        -> 返回 serverInfo + capabilities
/// - "notifications/*"   -> 返回 None（无需响应）
/// - "tools/list"        -> 返回静态工具列表（分页支持）
/// - "tools/call"        -> 转发至 Daemon
/// - "resources/list"    -> 返回 {"resources":[]}
/// - "prompts/list"      -> 返回 {"prompts":[]}
/// - "ping"              -> 返回 {}
/// - "logging/setLevel"  -> 返回 {}
/// - 其他                -> error -32601 Method not found
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
        // tools/list: 返回工具列表（来自 tools::all_tool_schemas()）
        // 参考: cccc/src/cccc/ports/mcp/main.py:135-147
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
/// - _group_id: GhostCode 工作组 ID（目前通过环境变量传递，参数备用）
/// - _actor_id: Actor ID（目前通过环境变量传递，参数备用）
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonrpc::JsonRpcRequest;

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
    async fn tools_list_returns_11() {
        let req = make_req("tools/list", serde_json::json!(2), serde_json::json!({}));
        let mut state = SessionState::default();
        let daemon_addr = std::path::Path::new("/nonexistent/daemon.sock");
        let resp = handle_request(req, &mut state, daemon_addr).await;

        let resp = resp.expect("tools/list 必须返回响应");
        let result = resp.result.expect("tools/list 必须有 result");
        let tools = result["tools"].as_array().expect("tools 必须是数组");
        assert_eq!(tools.len(), 16, "工具数量必须恰好为 16");
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
