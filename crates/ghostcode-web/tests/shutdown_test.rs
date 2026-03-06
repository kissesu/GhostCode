//! HTTP Server 启动与关停测试
//!
//! 验证 HTTP 服务器能正确启动、响应请求、并在收到关停信号后优雅退出。
//! 测试使用 lib crate 的 create_router + build_cors_layer 直接构建 app，
//! 通过 axum::serve + with_graceful_shutdown 验证实际行为。
//!
//! @author Atlas.oi
//! @date 2026-03-05

use axum::Router;
use ghostcode_web::server::{build_cors_layer, create_router};
use ghostcode_web::state::WebState;
use std::path::PathBuf;
use tokio::sync::oneshot;
use tokio::time::{timeout, Duration};

/// 创建测试用的 WebState（使用临时目录）
fn make_test_state() -> WebState {
    WebState::with_socket(
        PathBuf::from("/tmp/ghostcode-test"),
        PathBuf::from("/tmp/ghostcode-test/daemon.sock"),
    )
}

/// 构建测试用 app（含 CORS 层）
fn build_test_app() -> Router {
    let state = make_test_state();
    let cors = build_cors_layer(&["http://localhost:3000".to_string()]);
    create_router(state).layer(cors)
}

/// 测试 1：服务器启动后 GET /health 返回 200
///
/// 验证逻辑：
/// 1. 绑定端口（:0 自动分配）
/// 2. 启动 server（通过 oneshot 关停信号控制生命周期）
/// 3. 发送 GET /health 请求
/// 4. 验证返回 200
/// 5. 发送关停信号，验证 server 在超时内退出
#[tokio::test]
async fn test_health_returns_200() {
    // 绑定随机端口
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定端口失败");
    let addr = listener.local_addr().expect("获取本地地址失败");

    // 创建关停信号 channel
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    // 在后台 task 中启动 server
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, build_test_app())
            .with_graceful_shutdown(async {
                // 收到 shutdown 信号后进入优雅关停
                let _ = shutdown_rx.await;
            })
            .await
            .expect("server 异常退出")
    });

    // 等待 server 就绪（短暂延迟）
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 发送 GET /health 请求
    let url = format!("http://{}/health", addr);
    let resp = reqwest::get(&url).await.expect("请求失败");

    // 验证返回 200
    assert_eq!(resp.status().as_u16(), 200, "GET /health 应返回 200");

    // 发送关停信号
    shutdown_tx.send(()).expect("发送关停信号失败");

    // 验证 server 在 2 秒内退出
    let result = timeout(Duration::from_secs(2), server_handle).await;
    assert!(result.is_ok(), "server 应在 2 秒内退出");
    result.unwrap().expect("server task 不应 panic");
}

/// 测试 2：关停信号触发后 server 在 grace period 内退出
///
/// 验证逻辑：
/// 1. 启动 server
/// 2. 立即发送关停信号（无活跃连接）
/// 3. 验证 server 在 1 秒内退出（无连接时应立即退出）
#[tokio::test]
async fn test_shutdown_signal_triggers_exit() {
    // 绑定随机端口
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定端口失败");

    // 创建关停信号 channel
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    // 在后台 task 中启动 server
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, build_test_app())
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("server 异常退出")
    });

    // 等待 server 就绪
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 立即发送关停信号
    shutdown_tx.send(()).expect("发送关停信号失败");

    // 验证 server 在 1 秒内退出（无活跃连接时应快速退出）
    let result = timeout(Duration::from_secs(1), server_handle).await;
    assert!(result.is_ok(), "无活跃连接时 server 应在 1 秒内退出");
    result.unwrap().expect("server task 不应 panic");
}

/// 测试 3：验证 with_graceful_shutdown 接受 oneshot channel 作为 shutdown future
///
/// 验证逻辑：
/// 1. 确认 axum::serve.with_graceful_shutdown 可接受 oneshot::Receiver
/// 2. 这实际上验证了 main.rs 中使用的模式是正确的
/// 3. 验证两个不同 server 实例可以独立控制关停
#[tokio::test]
async fn test_injectable_shutdown_future() {
    // 创建两个独立 server，各自有独立的关停信号
    let listener_a = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定端口 A 失败");
    let listener_b = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定端口 B 失败");

    let (tx_a, rx_a) = oneshot::channel::<()>();
    let (tx_b, rx_b) = oneshot::channel::<()>();

    let handle_a = tokio::spawn(async move {
        axum::serve(listener_a, build_test_app())
            .with_graceful_shutdown(async { let _ = rx_a.await; })
            .await
            .unwrap()
    });

    let handle_b = tokio::spawn(async move {
        axum::serve(listener_b, build_test_app())
            .with_graceful_shutdown(async { let _ = rx_b.await; })
            .await
            .unwrap()
    });

    // 等待两个 server 就绪
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 先关停 A，B 仍在运行
    tx_a.send(()).expect("发送关停信号 A 失败");
    let result_a = timeout(Duration::from_secs(1), handle_a).await;
    assert!(result_a.is_ok(), "server A 应在 1 秒内退出");

    // 再关停 B
    tx_b.send(()).expect("发送关停信号 B 失败");
    let result_b = timeout(Duration::from_secs(1), handle_b).await;
    assert!(result_b.is_ok(), "server B 应在 1 秒内退出");
}
