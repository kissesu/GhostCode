/**
 * @file main_cli_test.rs
 * @description ghostcode-web CLI 参数解析集成测试
 *   通过在测试文件内联 Args 结构体定义契约，验证 clap 参数解析行为。
 *   main.rs 的实现必须与此契约保持一致。
 * @author Atlas.oi
 * @date 2026-03-05
 */

use std::net::SocketAddr;
use std::path::PathBuf;
use clap::Parser;
use proptest::prelude::*;

// ============================================
// 内联 Args 契约定义
// 由于 bin target 的类型无法从外部 crate 导入，
// 这里重新定义相同的 Args 结构体来测试 clap 解析行为。
// main.rs 的实现必须与此契约保持一致。
// ============================================

#[derive(Parser, Debug, Clone)]
#[command(name = "ghostcode-web")]
struct Args {
    /// HTTP 服务器绑定地址
    #[arg(long, default_value = "127.0.0.1:7070")]
    bind: SocketAddr,

    /// 数据根目录
    #[arg(long, value_name = "DIR")]
    base_dir: Option<PathBuf>,

    /// Daemon Unix Socket 路径
    #[arg(long, value_name = "SOCKET")]
    daemon_socket: Option<PathBuf>,

    /// CORS 允许的源（可多次指定）
    #[arg(long, default_value = "http://localhost:5173")]
    cors_origin: Vec<String>,

    /// 优雅关停等待秒数
    #[arg(long, default_value = "10")]
    shutdown_grace_secs: u64,
}

// ============================================
// 基础默认值测试
// ============================================

#[test]
fn test_main_cli_default_bind_address() {
    // 验证默认绑定地址为 127.0.0.1:7070
    let args = Args::try_parse_from(["ghostcode-web"]).unwrap();
    let expected: SocketAddr = "127.0.0.1:7070".parse().unwrap();
    assert_eq!(args.bind, expected, "默认绑定地址应为 127.0.0.1:7070");
}

#[test]
fn test_main_cli_default_cors_origin() {
    // 验证默认 cors_origin 包含 http://localhost:5173
    let args = Args::try_parse_from(["ghostcode-web"]).unwrap();
    assert!(
        args.cors_origin.contains(&"http://localhost:5173".to_string()),
        "默认 cors_origin 应包含 http://localhost:5173，实际为: {:?}",
        args.cors_origin
    );
}

#[test]
fn test_main_cli_default_shutdown_grace_secs() {
    // 验证默认优雅关停时间为 10 秒
    let args = Args::try_parse_from(["ghostcode-web"]).unwrap();
    assert_eq!(args.shutdown_grace_secs, 10, "默认关停等待时间应为 10 秒");
}

#[test]
fn test_main_cli_default_base_dir_is_none() {
    // 验证默认 base_dir 为 None（由 main.rs 运行时填充默认路径）
    let args = Args::try_parse_from(["ghostcode-web"]).unwrap();
    assert!(args.base_dir.is_none(), "默认 base_dir 应为 None");
}

#[test]
fn test_main_cli_default_daemon_socket_is_none() {
    // 验证默认 daemon_socket 为 None（由 main.rs 运行时从 base_dir 派生）
    let args = Args::try_parse_from(["ghostcode-web"]).unwrap();
    assert!(args.daemon_socket.is_none(), "默认 daemon_socket 应为 None");
}

// ============================================
// 参数覆盖测试
// ============================================

#[test]
fn test_main_cli_bind_override() {
    // 验证 --bind 参数能覆盖默认绑定地址
    let args = Args::try_parse_from(["ghostcode-web", "--bind", "0.0.0.0:8080"]).unwrap();
    let expected: SocketAddr = "0.0.0.0:8080".parse().unwrap();
    assert_eq!(args.bind, expected, "--bind 参数应覆盖默认地址");
}

#[test]
fn test_main_cli_base_dir_override() {
    // 验证 --base-dir 参数能指定数据目录
    let args =
        Args::try_parse_from(["ghostcode-web", "--base-dir", "/tmp/my-ghostcode"]).unwrap();
    assert_eq!(
        args.base_dir,
        Some(PathBuf::from("/tmp/my-ghostcode")),
        "--base-dir 参数应设置数据目录"
    );
}

#[test]
fn test_main_cli_daemon_socket_override() {
    // 验证 --daemon-socket 参数能指定 socket 路径
    let args = Args::try_parse_from([
        "ghostcode-web",
        "--daemon-socket",
        "/tmp/my-daemon.sock",
    ])
    .unwrap();
    assert_eq!(
        args.daemon_socket,
        Some(PathBuf::from("/tmp/my-daemon.sock")),
        "--daemon-socket 参数应设置 socket 路径"
    );
}

#[test]
fn test_main_cli_cors_origin_multiple() {
    // 验证 --cors-origin 可以指定多个源
    let args = Args::try_parse_from([
        "ghostcode-web",
        "--cors-origin",
        "http://localhost:3000",
        "--cors-origin",
        "https://app.example.com",
    ])
    .unwrap();
    assert!(
        args.cors_origin.contains(&"http://localhost:3000".to_string()),
        "cors_origin 应包含 http://localhost:3000"
    );
    assert!(
        args.cors_origin.contains(&"https://app.example.com".to_string()),
        "cors_origin 应包含 https://app.example.com"
    );
}

#[test]
fn test_main_cli_shutdown_grace_secs_override() {
    // 验证 --shutdown-grace-secs 能覆盖默认关停等待时间
    let args =
        Args::try_parse_from(["ghostcode-web", "--shutdown-grace-secs", "30"]).unwrap();
    assert_eq!(args.shutdown_grace_secs, 30, "--shutdown-grace-secs 应覆盖默认值");
}

// ============================================
// 基于属性的测试（PBT）
// 验证随机端口号（1024-65535）均能正确解析为 SocketAddr
// ============================================

proptest! {
    #[test]
    fn test_main_cli_pbt_random_port_parses_correctly(port in 1024u16..=65535u16) {
        // 随机端口号应该都能正确解析为有效的 SocketAddr
        let bind_str = format!("127.0.0.1:{}", port);
        let args = Args::try_parse_from(["ghostcode-web", "--bind", &bind_str]).unwrap();
        assert_eq!(args.bind.port(), port, "端口 {} 应该被正确解析", port);
        assert_eq!(
            args.bind.ip().to_string(),
            "127.0.0.1",
            "IP 地址应为 127.0.0.1"
        );
    }
}
