//! Phase 1.5 Daemon 启动链路测试（TDD Red 阶段）
//!
//! 测试 startup.rs 中的 run_daemon 函数
//! 验证完整启动链路：锁 -> 路径 -> 清理 -> AppState -> addr -> PID -> serve -> shutdown
//!
//! 测试用例清单：
//! 1. test_startup_creates_addr_json  - 启动后 addr.json 正确生成
//! 2. test_startup_creates_pid_file   - 启动后 PID 文件正确生成
//! 3. test_startup_acquires_lock      - 启动后锁被持有，二次获取失败
//! 4. test_startup_cleans_stale_files - 启动前清理残留文件
//! 5. test_startup_responds_to_ping   - 启动后可通过 socket 收到 pong 响应
//! 6. test_shutdown_cleans_up         - shutdown 后 socket 文件被清理
//!
//! @author Atlas.oi
//! @date 2026-03-02

use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use ghostcode_daemon::lock::try_acquire_singleton_lock;
use ghostcode_daemon::paths::DaemonPaths;
use ghostcode_daemon::process::read_addr_descriptor;
// 注意：startup 模块将在 Task 2（TDD Green 阶段）中创建
// 此处引用预期会导致编译失败，属于 TDD Red 阶段预期行为
use ghostcode_daemon::startup::{run_daemon, StartupConfig};
use ghostcode_types::ipc::DaemonRequest;

// ============================================
// 测试辅助函数
// ============================================

/// 在后台启动 Daemon 并等待就绪
///
/// 业务逻辑：
/// 1. 根据 base_dir 构建 DaemonPaths
/// 2. 构造 StartupConfig 并在 tokio::spawn 中启动 run_daemon
/// 3. 轮询等待 addr.json 出现（最多 3 秒）
///
/// @param base_dir - 临时测试目录
/// @return (JoinHandle, DaemonPaths) - 任务句柄和路径管理器
async fn start_daemon_in_background(
    base_dir: &Path,
) -> (tokio::task::JoinHandle<()>, DaemonPaths) {
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

    // 等待 addr.json 出现且包含有效内容（最多 3 秒，每 100ms 轮询一次）
    //
    // 仅检查文件是否存在不够健壮：可能存在 stale 文件或尚未完整写入的文件。
    // 通过验证 JSON 内容中 v=1 字段，确认 Daemon 完成了初始化并写入了有效的描述符。
    for _ in 0..30 {
        if let Ok(content) = std::fs::read_to_string(&paths.addr) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                if val["v"] == 1 {
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    (handle, paths)
}

/// 通过 UnixStream 发送 DaemonRequest 并读取第一行响应
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

/// 测试 1：Daemon 启动后应创建 addr.json 文件
///
/// 验证 addr.json 文件存在，且内容中包含合法的 v=1, transport="unix" 字段
#[tokio::test]
async fn test_startup_creates_addr_json() {
    let dir = tempfile::TempDir::new().unwrap();
    let (_handle, paths) = start_daemon_in_background(dir.path()).await;

    // addr.json 应在 3 秒内出现
    assert!(
        paths.addr.exists(),
        "addr.json 应在 Daemon 启动后被创建: {}",
        paths.addr.display()
    );

    // 验证 addr.json 内容格式正确
    let descriptor = read_addr_descriptor(&paths.addr)
        .expect("读取 addr.json 不应失败")
        .expect("addr.json 存在时应返回 Some");

    assert_eq!(descriptor.v, 1, "addr.json v 字段应为 1");
    assert_eq!(
        descriptor.transport, "unix",
        "addr.json transport 字段应为 unix"
    );
    assert!(
        !descriptor.path.is_empty(),
        "addr.json path 字段不应为空"
    );
}

/// 测试 2：Daemon 启动后应创建 PID 文件
///
/// 验证 PID 文件存在，且内容为合法的 u32 进程 ID
#[tokio::test]
async fn test_startup_creates_pid_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let (_handle, paths) = start_daemon_in_background(dir.path()).await;

    // PID 文件应在 3 秒内出现
    assert!(
        paths.pid.exists(),
        "PID 文件应在 Daemon 启动后被创建: {}",
        paths.pid.display()
    );

    // 读取 PID 文件内容并解析为 u32
    let pid_content = std::fs::read_to_string(&paths.pid).expect("读取 PID 文件不应失败");
    let pid: u32 = pid_content
        .trim()
        .parse()
        .expect("PID 文件内容应为合法的 u32 数字");

    assert!(pid > 0, "PID 应为正整数，当前值: {}", pid);
}

/// 测试 3：Daemon 启动后应持有单实例锁，二次获取应返回 AlreadyRunning
///
/// 验证启动期间无法再次获取同一个锁文件的排他锁
#[tokio::test]
async fn test_startup_acquires_lock() {
    let dir = tempfile::TempDir::new().unwrap();
    let (_handle, paths) = start_daemon_in_background(dir.path()).await;

    // Daemon 运行中，尝试获取同一个锁文件的锁应失败
    let lock_result = try_acquire_singleton_lock(&paths.lock);

    match lock_result {
        Err(ghostcode_daemon::lock::LockError::AlreadyRunning) => {
            // 预期结果：锁已被 Daemon 持有
        }
        Ok(_) => {
            panic!("Daemon 运行中时，二次获取锁应返回 AlreadyRunning，但获取成功了");
        }
        Err(e) => {
            panic!("获取锁时发生意外错误: {}", e);
        }
    }
}

/// 测试 4：Daemon 启动前应清理残留文件
///
/// 预先在 daemon_dir 创建旧的 sock/addr/pid 文件
/// 启动后验证这些文件被新的有效内容替换
#[tokio::test]
async fn test_startup_cleans_stale_files() {
    let dir = tempfile::TempDir::new().unwrap();

    // 预先创建 daemon_dir 和残留文件
    let paths = DaemonPaths::new(dir.path());
    std::fs::create_dir_all(&paths.daemon_dir).unwrap();

    // 写入旧的残留内容（内容故意无效，用于区分）
    let stale_content = b"stale-content-from-previous-crash";
    std::fs::write(&paths.sock, stale_content).unwrap();
    std::fs::write(&paths.addr, stale_content).unwrap();
    std::fs::write(&paths.pid, stale_content).unwrap();

    // 启动 Daemon
    let (_handle, paths) = start_daemon_in_background(dir.path()).await;

    // 验证 addr.json 被新的有效内容替换（不再是旧的残留内容）
    let addr_content = std::fs::read_to_string(&paths.addr).expect("addr.json 应存在");
    assert!(
        !addr_content.contains("stale-content"),
        "addr.json 应被新内容替换，不应包含残留内容"
    );

    // 验证 addr.json 可被正确解析（新内容合法）
    let descriptor = read_addr_descriptor(&paths.addr)
        .expect("读取 addr.json 不应失败")
        .expect("addr.json 应包含有效的描述符");
    assert_eq!(descriptor.v, 1, "新 addr.json 的 v 字段应为 1");

    // 验证 PID 文件被新的有效内容替换
    let pid_content = std::fs::read_to_string(&paths.pid).expect("PID 文件应存在");
    assert!(
        !pid_content.contains("stale-content"),
        "PID 文件应被新内容替换"
    );
    assert!(
        pid_content.trim().parse::<u32>().is_ok(),
        "PID 文件应包含合法的 u32 数字"
    );
}

/// 测试 5：Daemon 启动后应通过 socket 响应 ping 请求，返回 pong=true
///
/// 验证完整的 IPC 通信链路：UnixStream 连接 -> 发送 ping -> 收到 pong 响应
#[tokio::test]
async fn test_startup_responds_to_ping() {
    let dir = tempfile::TempDir::new().unwrap();
    let (_handle, paths) = start_daemon_in_background(dir.path()).await;

    // 验证 addr.json 存在并读取 socket 路径
    assert!(paths.addr.exists(), "addr.json 应在 Daemon 启动后存在");

    let descriptor = read_addr_descriptor(&paths.addr)
        .expect("读取 addr.json 不应失败")
        .expect("addr.json 应包含有效的描述符");

    // 通过 addr.json 中记录的 socket 路径连接
    let sock_path = std::path::PathBuf::from(&descriptor.path);

    // 发送 ping 请求
    let req = DaemonRequest::new("ping", serde_json::json!({}));
    let response = send_request(&sock_path, &req).await;

    // 验证响应包含 pong=true
    let resp: serde_json::Value =
        serde_json::from_str(response.trim()).expect("响应应为合法 JSON");
    assert_eq!(resp["ok"], true, "ping 响应的 ok 字段应为 true");
    assert_eq!(
        resp["result"]["pong"], true,
        "ping 响应的 result.pong 字段应为 true"
    );
}

/// 测试 6：发送 shutdown 请求后，Daemon 应退出并清理 socket 文件
///
/// 验证 shutdown 操作的完整链路：发送 shutdown -> run_daemon 退出 -> socket 文件消失
#[tokio::test]
async fn test_shutdown_cleans_up() {
    let dir = tempfile::TempDir::new().unwrap();
    let (handle, paths) = start_daemon_in_background(dir.path()).await;

    // 确认 Daemon 已启动
    assert!(paths.sock.exists(), "socket 文件应在 Daemon 启动后存在");
    assert!(paths.addr.exists(), "addr.json 应在 Daemon 启动后存在");

    // 读取 socket 路径
    let descriptor = read_addr_descriptor(&paths.addr)
        .expect("读取 addr.json 不应失败")
        .expect("addr.json 应包含有效的描述符");
    let sock_path = std::path::PathBuf::from(&descriptor.path);

    // 发送 shutdown 请求（忽略响应，因为连接会在 shutdown 时断开）
    let req = DaemonRequest::new("shutdown", serde_json::json!({}));
    let _ = send_request(&sock_path, &req).await;

    // 等待 run_daemon 任务退出（最多 5 秒）
    let shutdown_result = tokio::time::timeout(Duration::from_secs(5), handle).await;
    assert!(
        shutdown_result.is_ok(),
        "run_daemon 应在 shutdown 请求后 5 秒内退出"
    );

    // 验证 socket 文件在关闭后被清理
    // 稍等一下确保清理完成
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        !paths.sock.exists(),
        "socket 文件应在 Daemon 关闭后被清理: {}",
        paths.sock.display()
    );
}
