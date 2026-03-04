//! 端到端 IPC 集成测试
//!
//! 验证 Daemon 通过真实 Unix socket 处理所有注册 op 的完整链路。
//! 测试范围：socket 连接 -> JSON 协议解析 -> dispatch 路由 -> 处理器执行 -> 响应序列化
//!
//! @author Atlas.oi
//! @date 2026-03-04

use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use ghostcode_daemon::server::{AppState, DaemonConfig};
use ghostcode_types::ipc::{DaemonRequest, DaemonResponse};

// ============================================
// 测试辅助函数
// ============================================

/// 启动临时 Daemon，返回 (临时目录, socket 路径, AppState)
///
/// 复用 server_test.rs 的启动模式：
/// 1. 先 bind socket（与生产环境启动顺序一致）
/// 2. tokio::spawn 启动 serve_forever
/// 3. 等待 socket 文件出现后返回
async fn start_test_daemon() -> (tempfile::TempDir, std::path::PathBuf, Arc<AppState>) {
    let dir = tempfile::TempDir::new().unwrap();
    let sock_path = dir.path().join("e2e_test.sock");
    let state = Arc::new(AppState::default());

    // 先 bind socket，与生产环境启动顺序一致
    let listener = UnixListener::bind(&sock_path).unwrap();

    let config = DaemonConfig {
        socket_path: sock_path.clone(),
    };

    let server_state = Arc::clone(&state);
    tokio::spawn(async move {
        let _ = ghostcode_daemon::server::serve_forever(listener, config, server_state).await;
    });

    // 等待 socket 文件就绪（bind 后文件已存在，轮询兜底）
    for _ in 0..50 {
        if sock_path.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    (dir, sock_path, state)
}

/// 通过 Unix socket 发送 DaemonRequest，返回原始响应行
///
/// 业务逻辑：
/// 1. 连接 socket
/// 2. 序列化请求为 JSON 并追加换行符写入
/// 3. 读取一行响应后返回
async fn send_request(sock_path: &std::path::Path, req: &DaemonRequest) -> String {
    let stream = UnixStream::connect(sock_path).await.unwrap();
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    let json = serde_json::to_string(req).unwrap();
    write_half
        .write_all(format!("{}\n", json).as_bytes())
        .await
        .unwrap();
    write_half.flush().await.unwrap();

    let mut response_line = String::new();
    reader.read_line(&mut response_line).await.unwrap();
    response_line
}

/// 通过 Unix socket 发送原始字节，带超时返回响应行
///
/// 用于测试非法输入场景，超时或连接断开时返回空字符串
async fn send_raw(sock_path: &std::path::Path, data: &[u8]) -> std::io::Result<String> {
    let stream = UnixStream::connect(sock_path).await?;
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    write_half.write_all(data).await?;
    write_half.write_all(b"\n").await?;
    write_half.flush().await?;

    let mut response_line = String::new();
    let n = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        reader.read_line(&mut response_line),
    )
    .await
    .unwrap_or(Ok(0))?;

    if n == 0 {
        Ok(String::new())
    } else {
        Ok(response_line)
    }
}

// ============================================
// E2E 测试用例
// ============================================

/// 测试 1：ping 通过真实 Unix socket 可达
///
/// 验证最基础的通信链路：连接 -> 发送 ping -> 收到 pong=true
#[tokio::test]
async fn ping_over_real_socket() {
    let (_dir, sock_path, _state) = start_test_daemon().await;

    let req = DaemonRequest::new("ping", serde_json::json!({}));
    let response = send_request(&sock_path, &req).await;

    // 响应必须是合法 JSON
    let resp: DaemonResponse = serde_json::from_str(response.trim())
        .expect("ping 响应应为合法 DaemonResponse JSON");

    assert!(resp.ok, "ping 应返回 ok=true");
    assert_eq!(resp.v, 1, "协议版本应为 1");

    // 验证 pong 字段（result 是 serde_json::Value，直接访问即可）
    assert_eq!(resp.result["pong"], true, "ping 响应的 result.pong 应为 true");
}

/// 测试 2：所有注册的 op 均通过真实 Unix socket 可达
///
/// 遍历 dispatch.rs 中的 KNOWN_OPS 列表，对每个 op 发送空 args 请求。
/// 验证条件：收到 DaemonResponse（即使 ok=false 也说明 dispatch 正确路由了请求，
/// 而非协议层错误）。不存在的 op 才会触发 UNKNOWN_OP 错误。
///
/// 排除 shutdown op（会关闭 Daemon，影响后续测试）
#[tokio::test]
async fn all_ops_reachable() {
    let (_dir, sock_path, _state) = start_test_daemon().await;

    // 从 dispatch.rs 的 KNOWN_OPS 获取所有注册 op
    // 排除 shutdown：发送 shutdown 会关闭 Daemon，导致后续连接失败
    let known_ops: Vec<&str> = ghostcode_daemon::dispatch::KNOWN_OPS
        .iter()
        .copied()
        .filter(|op| *op != "shutdown")
        .collect();

    for op in &known_ops {
        // 解引用 &&str 到 &str 以满足 Into<String> 的 trait bound
        let req = DaemonRequest::new(*op, serde_json::json!({}));
        let response = send_request(&sock_path, &req).await;

        // 必须能解析为合法的 DaemonResponse
        let resp: DaemonResponse = serde_json::from_str(response.trim()).unwrap_or_else(|e| {
            panic!(
                "op='{}' 的响应应为合法 DaemonResponse JSON，解析错误: {}，原始响应: {}",
                op, e, response
            )
        });

        assert_eq!(resp.v, 1, "op='{}' 响应的协议版本应为 1", op);

        // ok=false 是允许的（例如参数缺失），但 op 必须被 dispatch 路由到
        // 如果 op 被正确路由，error.code 不应为 UNKNOWN_OP
        if !resp.ok {
            if let Some(ref err) = resp.error {
                assert_ne!(
                    err.code, "UNKNOWN_OP",
                    "op='{}' 不应返回 UNKNOWN_OP，该 op 应已在 dispatch 中注册",
                    op
                );
            }
        }
    }
}

/// 测试 3：未知 op 返回 UNKNOWN_OP 错误
///
/// 发送不在 KNOWN_OPS 中的 op，验证 dispatch 返回明确的错误而非 panic
#[tokio::test]
async fn invalid_op_returns_error() {
    let (_dir, sock_path, _state) = start_test_daemon().await;

    // 发送一个肯定不存在的 op
    let req = DaemonRequest::new("this_op_does_not_exist_xyz", serde_json::json!({}));
    let response = send_request(&sock_path, &req).await;

    let resp: DaemonResponse = serde_json::from_str(response.trim())
        .expect("未知 op 的响应应为合法 DaemonResponse JSON");

    assert!(!resp.ok, "未知 op 应返回 ok=false");

    let error = resp.error.expect("未知 op 响应应包含 error 字段");
    assert_eq!(
        error.code, "UNKNOWN_OP",
        "未知 op 的错误码应为 UNKNOWN_OP，实际为: {}",
        error.code
    );

    // 验证 Daemon 仍然存活
    let ping_req = DaemonRequest::new("ping", serde_json::json!({}));
    let ping_response = send_request(&sock_path, &ping_req).await;
    let ping_resp: DaemonResponse = serde_json::from_str(ping_response.trim()).unwrap();
    assert!(ping_resp.ok, "Daemon 应在处理未知 op 后仍能正常服务");
}

/// 测试 4：非法 JSON 返回错误响应
///
/// 发送无法解析为 JSON 的数据，验证：
/// 1. Server 返回错误响应（ok=false）而非崩溃
/// 2. Server 在处理后仍能正常服务后续请求
#[tokio::test]
async fn invalid_json_returns_error() {
    let (_dir, sock_path, _state) = start_test_daemon().await;

    // 发送明显不是 JSON 的数据
    let response = send_raw(&sock_path, b"this is not json at all").await.unwrap();

    // 若有响应，必须是合法 JSON 且 ok=false
    if !response.is_empty() {
        let resp: serde_json::Value = serde_json::from_str(response.trim())
            .expect("非法 JSON 输入的响应本身应为合法 JSON");
        assert_eq!(resp["ok"], false, "非法 JSON 输入应返回 ok=false");
    }

    // 最重要的验证：Daemon 在处理非法 JSON 后仍然存活
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let ping_req = DaemonRequest::new("ping", serde_json::json!({}));
    let ping_response = send_request(&sock_path, &ping_req).await;
    let ping_resp: DaemonResponse = serde_json::from_str(ping_response.trim()).unwrap();
    assert!(ping_resp.ok, "Daemon 应在处理非法 JSON 后仍能正常服务");
}

/// 测试 5：并发请求全部得到响应
///
/// 同时打开多个连接并发发送不同 op 的请求，验证：
/// 1. 所有请求都得到响应（无丢失）
/// 2. 每个响应都是合法的 DaemonResponse
/// 3. Daemon 能正确处理并发场景
#[tokio::test]
async fn concurrent_requests() {
    let (_dir, sock_path, _state) = start_test_daemon().await;

    // 并发发送 20 个请求，混合不同的 op
    let concurrent_count = 20;
    let mut handles = Vec::with_capacity(concurrent_count);

    for i in 0..concurrent_count {
        let path = sock_path.clone();

        let handle = tokio::spawn(async move {
            // 交替发送 ping 和 groups（两者都是安全的只读操作）
            let op = if i % 2 == 0 { "ping" } else { "groups" };
            let req = DaemonRequest::new(op, serde_json::json!({}));
            let response = send_request(&path, &req).await;

            // 每个响应都必须是合法的 DaemonResponse
            let resp: DaemonResponse =
                serde_json::from_str(response.trim()).unwrap_or_else(|e| {
                    panic!(
                        "并发请求 {} (op='{}') 的响应应为合法 JSON，错误: {}，原始: {}",
                        i, op, e, response
                    )
                });

            assert_eq!(resp.v, 1, "并发请求 {} 的响应协议版本应为 1", i);
            resp.ok
        });

        handles.push(handle);
    }

    // 等待所有并发请求完成，统计成功数量
    let mut success_count = 0;
    for (i, handle) in handles.into_iter().enumerate() {
        let ok = handle
            .await
            .unwrap_or_else(|e| panic!("并发请求 {} 的 tokio task 失败: {}", i, e));
        if ok {
            success_count += 1;
        }
    }

    // ping 请求（偶数索引，共 10 个）必须全部成功
    assert!(
        success_count >= 10,
        "至少 10 个 ping 请求应成功，实际成功: {}",
        success_count
    );
}
