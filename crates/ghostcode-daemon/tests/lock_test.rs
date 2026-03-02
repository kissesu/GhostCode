//! ghostcode-daemon 锁和进程管理测试
//!
//! 覆盖 T05 TDD 规范定义的所有测试用例
//! - 单实例锁获取/互斥
//! - 端点描述符往返
//! - 残留文件清理
//! - Socket 路径长度检查 [ASSUM-2]
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_daemon::lock::try_acquire_singleton_lock;
use ghostcode_daemon::paths::DaemonPaths;
use ghostcode_daemon::process::{
    cleanup_stale_files, read_addr_descriptor, write_addr_descriptor,
};
use ghostcode_types::addr::AddrDescriptor;
use tempfile::TempDir;

// ============================================
// 单实例锁测试
// ============================================

#[test]
fn singleton_lock_acquired() {
    let dir = TempDir::new().unwrap();
    let lock_path = dir.path().join("daemon/ghostcoded.lock");

    // 第一次获取应成功
    let _lock = try_acquire_singleton_lock(&lock_path).unwrap();
}

#[test]
fn singleton_lock_exclusive() {
    let dir = TempDir::new().unwrap();
    let lock_path = dir.path().join("daemon/ghostcoded.lock");

    // 第一次获取成功
    let _lock1 = try_acquire_singleton_lock(&lock_path).unwrap();

    // 第二次获取应失败（AlreadyRunning）
    let result = try_acquire_singleton_lock(&lock_path);
    assert!(result.is_err(), "第二次获取锁应失败");

    match result.unwrap_err() {
        ghostcode_daemon::lock::LockError::AlreadyRunning => {}
        other => panic!("应返回 AlreadyRunning，实际: {:?}", other),
    }
}

// ============================================
// 端点描述符测试
// ============================================

#[test]
fn addr_descriptor_roundtrip() {
    let dir = TempDir::new().unwrap();
    let addr_path = dir.path().join("daemon/ghostcoded.addr.json");

    let descriptor = AddrDescriptor::new("/tmp/ghostcode.sock", 12345, "0.1.0");

    // write -> read -> 相等
    write_addr_descriptor(&addr_path, &descriptor).unwrap();
    let restored = read_addr_descriptor(&addr_path).unwrap().unwrap();

    assert_eq!(descriptor.v, restored.v);
    assert_eq!(descriptor.transport, restored.transport);
    assert_eq!(descriptor.path, restored.path);
    assert_eq!(descriptor.pid, restored.pid);
    assert_eq!(descriptor.version, restored.version);
    assert_eq!(descriptor.ts, restored.ts);
}

#[test]
fn read_nonexistent_addr_returns_none() {
    let dir = TempDir::new().unwrap();
    let addr_path = dir.path().join("does-not-exist.json");

    let result = read_addr_descriptor(&addr_path).unwrap();
    assert!(result.is_none());
}

// ============================================
// 残留文件清理测试
// ============================================

#[test]
fn cleanup_stale_files_removes_all() {
    let dir = TempDir::new().unwrap();
    let daemon_dir = dir.path().join("daemon");
    std::fs::create_dir_all(&daemon_dir).unwrap();

    // 创建假的残留文件
    std::fs::write(daemon_dir.join("ghostcoded.sock"), "fake socket").unwrap();
    std::fs::write(daemon_dir.join("ghostcoded.addr.json"), "{}").unwrap();
    std::fs::write(daemon_dir.join("ghostcoded.pid"), "12345").unwrap();

    // 清理
    cleanup_stale_files(&daemon_dir).unwrap();

    // 验证全部不存在
    assert!(!daemon_dir.join("ghostcoded.sock").exists());
    assert!(!daemon_dir.join("ghostcoded.addr.json").exists());
    assert!(!daemon_dir.join("ghostcoded.pid").exists());

    // 目录本身应保留
    assert!(daemon_dir.exists());
}

// ============================================
// Socket 路径长度检查 [ASSUM-2]
// ============================================

#[test]
fn socket_path_length_check() {
    // 正常路径：不触发后备
    let normal_paths = DaemonPaths::new(std::path::Path::new("/Users/test/.ghostcode"));
    assert!(
        !normal_paths.sock.to_string_lossy().starts_with("/tmp/ghostcode-"),
        "正常路径不应使用 /tmp 后备"
    );

    // 超长路径：触发 /tmp 后备
    let long_base = format!(
        "/Users/{}/very/deeply/nested/directory/structure",
        "a".repeat(80)
    );
    let long_paths = DaemonPaths::new(std::path::Path::new(&long_base));
    assert!(
        long_paths.sock.to_string_lossy().starts_with("/tmp/ghostcode-"),
        "超长路径应使用 /tmp 后备: {}",
        long_paths.sock.display()
    );

    // 后备路径应在长度限制内
    assert!(
        long_paths.sock.to_string_lossy().len() <= 100,
        "后备路径应 <= 100 字符: {} (长度 {})",
        long_paths.sock.display(),
        long_paths.sock.to_string_lossy().len()
    );
}
