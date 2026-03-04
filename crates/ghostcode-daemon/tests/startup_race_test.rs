//! Daemon 启动顺序竞态测试（TDD Red 阶段）
//!
//! 验证 run_daemon 严格遵守"先 bind socket -> 再写 addr.json"的顺序，
//! 消除客户端读到 addr.json 但 socket 尚未就绪的竞态窗口。
//!
//! 测试用例清单：
//! 1. addr_json_written_after_socket_bind  - addr.json 出现时 socket 必须已可连接
//! 2. shutdown_cleans_addr_before_socket   - 优雅关闭时先删 addr.json 再关 socket
//!
//! @author Atlas.oi
//! @date 2026-03-04

use std::path::Path;
use std::time::Duration;

use tokio::net::UnixStream;

use ghostcode_daemon::paths::DaemonPaths;
use ghostcode_daemon::startup::{run_daemon, StartupConfig};
use ghostcode_types::ipc::DaemonRequest;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

// ============================================
// 测试辅助函数
// ============================================

/// 在后台 tokio::spawn 中启动 Daemon
///
/// 业务逻辑：
/// 1. 构建 StartupConfig（使用临时目录）
/// 2. 在 tokio::spawn 中异步启动 run_daemon
/// 3. 不等待 addr.json 出现（由测试自己控制轮询时机）
///
/// @param base_dir - 临时测试目录
/// @return (JoinHandle, DaemonPaths)
fn spawn_daemon(base_dir: &Path) -> (tokio::task::JoinHandle<()>, DaemonPaths) {
    let paths = DaemonPaths::new(base_dir);
    let groups_dir = base_dir.join("groups");
    std::fs::create_dir_all(&groups_dir).unwrap();

    let config = StartupConfig {
        base_dir: base_dir.to_path_buf(),
        groups_dir,
    };

    let handle = tokio::spawn(async move {
        let _ = run_daemon(config).await;
    });

    (handle, paths)
}

/// 通过 UnixStream 向 Daemon 发送请求
///
/// @param sock_path - socket 文件路径
/// @param req - 要发送的请求
/// @return 响应的 JSON 字符串
async fn send_request(sock_path: &Path, req: &DaemonRequest) -> String {
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

// ============================================
// 测试用例
// ============================================

/// 测试 1：addr.json 出现时，socket 必须已可连接
///
/// 核心验证：消除竞态窗口
/// - 在 tokio::spawn 中启动 run_daemon
/// - 轮询等待 addr.json 出现（每 10ms 检查一次）
/// - addr.json 一旦出现，立即尝试连接 socket
/// - 连接必须立即成功，不允许出现 ConnectionRefused 错误
///
/// 如果 run_daemon 先写 addr.json 后 bind socket（旧实现），
/// 测试有概率失败：addr.json 存在但 socket 尚未就绪
#[tokio::test]
async fn addr_json_written_after_socket_bind() {
    let dir = tempfile::TempDir::new().unwrap();

    // ============================================
    // 第一步：确保 addr.json 不存在（避免残留干扰）
    // ============================================
    let paths = DaemonPaths::new(dir.path());
    let _ = std::fs::remove_file(&paths.addr);

    // ============================================
    // 第二步：在后台启动 Daemon，不等待就绪
    // ============================================
    let (_handle, paths) = spawn_daemon(dir.path());

    // ============================================
    // 第三步：轮询等待 addr.json 出现
    // 每 10ms 检查一次，最多等 3 秒（300 次）
    // ============================================
    let mut addr_appeared = false;
    for _ in 0..300 {
        if paths.addr.exists() {
            // 尝试读取并验证内容有效（非空文件）
            if let Ok(content) = std::fs::read_to_string(&paths.addr) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    if val["v"] == 1 {
                        addr_appeared = true;
                        break;
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert!(addr_appeared, "addr.json 应在 3 秒内出现");

    // ============================================
    // 第四步：addr.json 已出现，立即尝试连接 socket
    // 此时 socket 必须已经就绪可接受连接
    // ============================================
    let addr_content = std::fs::read_to_string(&paths.addr).unwrap();
    let addr_val: serde_json::Value = serde_json::from_str(&addr_content).unwrap();
    let sock_path_str = addr_val["path"].as_str().unwrap();
    let sock_path = std::path::PathBuf::from(sock_path_str);

    // 连接必须立即成功（不允许重试或超时等待）
    // 如果竞态存在，这里会出现 ConnectionRefused 错误导致 unwrap panic
    let connect_result = tokio::time::timeout(
        Duration::from_millis(500),
        UnixStream::connect(&sock_path),
    )
    .await;

    assert!(
        connect_result.is_ok(),
        "连接 socket 不应超时：addr.json 出现时 socket 应已就绪"
    );
    let stream_result = connect_result.unwrap();
    assert!(
        stream_result.is_ok(),
        "连接 socket 应成功：addr.json 出现时 socket 必须已 bind 完毕，错误: {:?}",
        stream_result.err()
    );
}

/// 测试 2：优雅关闭时先删 addr.json 再关 socket
///
/// 核心验证：关闭顺序正确性
/// - 启动 Daemon 并确认就绪
/// - 发送 shutdown 请求
/// - 在 Daemon 关闭过程中，持续检查 addr.json 和 socket 的状态
/// - 如果 addr.json 消失时 socket 仍存在 -> 符合预期（先删 addr 再关 socket）
/// - 如果 socket 消失时 addr.json 仍存在 -> 违反预期（关闭顺序错误）
///
/// 注意：由于关闭速度很快，此测试主要验证最终状态：
/// Daemon 退出后 addr.json 和 socket 都应被清理
#[tokio::test]
async fn shutdown_cleans_addr_before_socket() {
    let dir = tempfile::TempDir::new().unwrap();

    // ============================================
    // 第一步：启动 Daemon 并等待就绪
    // ============================================
    let paths = DaemonPaths::new(dir.path());
    let groups_dir = dir.path().join("groups");
    std::fs::create_dir_all(&groups_dir).unwrap();

    let config = StartupConfig {
        base_dir: dir.path().to_path_buf(),
        groups_dir,
    };

    let handle = tokio::spawn(async move {
        let _ = run_daemon(config).await;
    });

    // 等待 Daemon 就绪（addr.json 出现且内容有效）
    let mut ready = false;
    for _ in 0..300 {
        if let Ok(content) = std::fs::read_to_string(&paths.addr) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                if val["v"] == 1 {
                    ready = true;
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(ready, "Daemon 应在 3 秒内就绪");

    // 读取 socket 路径
    let addr_content = std::fs::read_to_string(&paths.addr).unwrap();
    let addr_val: serde_json::Value = serde_json::from_str(&addr_content).unwrap();
    let sock_path = std::path::PathBuf::from(addr_val["path"].as_str().unwrap());

    // ============================================
    // 第二步：发送 shutdown 请求
    // ============================================
    let req = DaemonRequest::new("shutdown", serde_json::json!({}));
    let _ = send_request(&sock_path, &req).await;

    // ============================================
    // 第三步：等待 Daemon 完全退出（最多 5 秒）
    // ============================================
    let result = tokio::time::timeout(Duration::from_secs(5), handle).await;
    assert!(
        result.is_ok(),
        "run_daemon 应在 shutdown 后 5 秒内退出"
    );

    // ============================================
    // 第四步：验证最终状态
    // Daemon 退出后 addr.json 和 socket 文件都应被清理
    // ============================================
    // 等待文件系统操作完成
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert!(
        !paths.addr.exists(),
        "Daemon 退出后 addr.json 应被清理: {}",
        paths.addr.display()
    );
    assert!(
        !sock_path.exists(),
        "Daemon 退出后 socket 文件应被清理: {}",
        sock_path.display()
    );
}
