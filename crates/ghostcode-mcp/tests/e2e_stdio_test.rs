// e2e_stdio_test.rs - bootstrap 模块单元测试 + E2E stdio 占位测试
//
// 测试 bootstrap::resolve_daemon_addr 函数的核心逻辑：
//   1. 给定包含有效 addr.json 的临时目录 -> 返回 addr.json 中的 path 字段
//   2. addr.json 不存在时 -> 返回 Err
//   3. addr.json 格式无效时 -> 返回 Err
//
// E2E stdio 测试（启动子进程通过 stdin/stdout 交互）依赖真实 Daemon，
// 暂时标记 #[ignore] 供手动运行。
//
// @author Atlas.oi
// @date 2026-03-04

use ghostcode_mcp::bootstrap;
use std::fs;
use std::path::PathBuf;

// ============================================
// 辅助函数：在临时目录内创建 daemon/ghostcoded.addr.json
// ============================================

/// 创建临时测试目录结构并写入 addr.json
///
/// @param base_dir - 临时目录路径
/// @param content  - addr.json 文件内容（JSON 字符串）
fn write_addr_json(base_dir: &std::path::Path, content: &str) {
    let daemon_dir = base_dir.join("daemon");
    fs::create_dir_all(&daemon_dir).expect("创建 daemon 子目录失败");
    fs::write(daemon_dir.join("ghostcoded.addr.json"), content)
        .expect("写入 addr.json 失败");
}

// ============================================
// 测试组 1：resolve_daemon_addr 正常路径
// ============================================

/// 测试：给定有效 addr.json，应返回其中的 "path" 字段值
#[test]
fn resolve_daemon_addr_returns_path_from_valid_addr_json() {
    // 创建临时目录
    let tmp_dir = tempfile::tempdir().expect("创建临时目录失败");
    let base_dir = tmp_dir.path();

    // 写入合法的 addr.json
    let sock_path = "/tmp/ghostcode_test/ghostcoded.sock";
    let addr_json = format!(
        r#"{{"v":1,"transport":"unix","path":"{}","pid":12345,"version":"0.1.0","ts":"2026-03-04T04:00:00Z"}}"#,
        sock_path
    );
    write_addr_json(base_dir, &addr_json);

    // 调用被测函数
    let result = bootstrap::resolve_daemon_addr(base_dir);

    // 验证返回路径与 addr.json 中的 path 字段一致
    assert!(result.is_ok(), "期望返回 Ok，实际: {:?}", result.err());
    assert_eq!(
        result.unwrap(),
        PathBuf::from(sock_path),
        "返回路径应与 addr.json 中的 path 字段一致"
    );
}

// ============================================
// 测试组 2：resolve_daemon_addr 错误路径
// ============================================

/// 测试：addr.json 不存在时应返回 Err
#[test]
fn resolve_daemon_addr_errors_when_file_missing() {
    // 创建临时目录但不写 addr.json
    let tmp_dir = tempfile::tempdir().expect("创建临时目录失败");
    let base_dir = tmp_dir.path();

    // daemon 子目录也不创建，模拟完全缺失的情况
    let result = bootstrap::resolve_daemon_addr(base_dir);

    assert!(
        result.is_err(),
        "addr.json 不存在时应返回 Err，实际: {:?}",
        result.ok()
    );

    // 错误消息应包含有用信息
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("addr.json") || err_msg.contains("无法读取"),
        "错误消息应提及 addr.json，实际: {}",
        err_msg
    );
}

/// 测试：addr.json 格式无效（非 JSON）时应返回 Err
#[test]
fn resolve_daemon_addr_errors_when_json_invalid() {
    let tmp_dir = tempfile::tempdir().expect("创建临时目录失败");
    let base_dir = tmp_dir.path();

    // 写入非 JSON 内容
    write_addr_json(base_dir, "这不是有效的 JSON 格式 !!!");

    let result = bootstrap::resolve_daemon_addr(base_dir);

    assert!(
        result.is_err(),
        "addr.json 格式无效时应返回 Err，实际: {:?}",
        result.ok()
    );
}

/// 测试：addr.json 是合法 JSON 但缺少 "path" 字段时应返回 Err
#[test]
fn resolve_daemon_addr_errors_when_path_field_missing() {
    let tmp_dir = tempfile::tempdir().expect("创建临时目录失败");
    let base_dir = tmp_dir.path();

    // 写入缺少 path 字段的 JSON
    write_addr_json(
        base_dir,
        r#"{"v":1,"transport":"unix","pid":12345}"#,
    );

    let result = bootstrap::resolve_daemon_addr(base_dir);

    assert!(
        result.is_err(),
        "缺少 path 字段时应返回 Err，实际: {:?}",
        result.ok()
    );

    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("path"),
        "错误消息应提及缺少 path 字段，实际: {}",
        err_msg
    );
}

// ============================================
// 测试组 3：default_base_dir
// ============================================

/// 测试：default_base_dir 返回路径应以 .ghostcode 结尾
#[test]
fn default_base_dir_ends_with_ghostcode() {
    let base_dir = bootstrap::default_base_dir();
    let dir_str = base_dir.to_string_lossy();

    assert!(
        dir_str.ends_with(".ghostcode"),
        "default_base_dir 应以 .ghostcode 结尾，实际: {}",
        dir_str
    );
}

/// 测试：设置 HOME 环境变量后，default_base_dir 应使用该路径
#[test]
fn default_base_dir_uses_home_env() {
    // 临时设置 HOME 环境变量
    let original_home = std::env::var("HOME").unwrap_or_default();
    std::env::set_var("HOME", "/custom/home");

    let base_dir = bootstrap::default_base_dir();
    let expected = PathBuf::from("/custom/home/.ghostcode");

    // 恢复原始 HOME
    std::env::set_var("HOME", &original_home);

    assert_eq!(
        base_dir, expected,
        "HOME=/custom/home 时，default_base_dir 应返回 /custom/home/.ghostcode"
    );
}

// ============================================
// E2E stdio 测试（依赖真实 Daemon，标记 ignore）
// ============================================

/// E2E 测试：启动 ghostcode-mcp 子进程，发送 JSON-RPC initialize 请求
///
/// 此测试需要：
///   1. ghostcode-mcp 二进制已构建（cargo build -p ghostcode-mcp）
///   2. GhostCode Daemon 已运行并生成有效的 addr.json
///
/// 标记 #[ignore] 供手动运行：cargo test -p ghostcode-mcp e2e_stdio -- --ignored
#[test]
#[ignore = "需要真实 Daemon 运行，手动执行：cargo test e2e_stdio -- --ignored"]
fn e2e_stdio_initialize_handshake() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // 构建 ghostcode-mcp 二进制路径
    let binary = env!("CARGO_BIN_EXE_ghostcode-mcp");

    // JSON-RPC initialize 请求
    let init_request = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}
"#;

    // 启动子进程
    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("启动 ghostcode-mcp 子进程失败");

    // 发送请求
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(init_request.as_bytes()).expect("写入 stdin 失败");
    }

    // 等待响应（超时 5 秒）
    let output = child.wait_with_output().expect("等待子进程失败");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(r#""result""#),
        "initialize 响应应包含 result 字段，实际: {}",
        stdout
    );
}
