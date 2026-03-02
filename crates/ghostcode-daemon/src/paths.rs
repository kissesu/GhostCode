//! Daemon 路径管理
//!
//! DaemonPaths 结构体集中管理所有 Daemon 相关的文件路径
//! 避免路径字符串分散在代码各处
//! 包含 Unix Socket 路径长度检查和 /tmp 后备逻辑 [ASSUM-2]
//!
//! @author Atlas.oi
//! @date 2026-03-01

use std::path::{Path, PathBuf};

/// macOS Unix Socket 路径长度限制
/// macOS sockaddr_un.sun_path 最大 104 字符
const SOCKET_PATH_MAX_LEN: usize = 100;

/// Daemon 路径管理器
///
/// 集中管理所有 Daemon 相关文件路径：
/// - lock: 单实例锁文件
/// - sock: Unix Socket 文件
/// - addr: 端点描述符 JSON 文件
/// - pid: 进程 ID 文件
/// - log: 日志文件
///
/// 当 socket 路径超过 100 字符时 [ASSUM-2]
/// 自动切换到 /tmp/ghostcode-<hash>/ 后备路径
#[derive(Debug, Clone)]
pub struct DaemonPaths {
    /// daemon 目录（通常为 ~/.ghostcode/daemon/）
    pub daemon_dir: PathBuf,
    /// 单实例锁文件
    pub lock: PathBuf,
    /// Unix Socket 文件
    pub sock: PathBuf,
    /// 端点描述符 JSON
    pub addr: PathBuf,
    /// PID 文件
    pub pid: PathBuf,
    /// 日志文件
    pub log: PathBuf,
}

impl DaemonPaths {
    /// 从基础目录创建 DaemonPaths
    ///
    /// 自动检查 socket 路径长度，超长时使用 /tmp 后备
    ///
    /// @param base_dir - 基础目录（如 ~/.ghostcode/）
    pub fn new(base_dir: &Path) -> Self {
        let daemon_dir = base_dir.join("daemon");
        let sock_path = daemon_dir.join("ghostcoded.sock");

        // [ASSUM-2] 检查 socket 路径长度
        let effective_daemon_dir = if sock_path.to_string_lossy().len() > SOCKET_PATH_MAX_LEN {
            // 使用 /tmp 后备路径
            let hash = simple_hash(&daemon_dir.to_string_lossy());
            PathBuf::from(format!("/tmp/ghostcode-{}", hash))
        } else {
            daemon_dir
        };

        Self {
            lock: effective_daemon_dir.join("ghostcoded.lock"),
            sock: effective_daemon_dir.join("ghostcoded.sock"),
            addr: effective_daemon_dir.join("ghostcoded.addr.json"),
            pid: effective_daemon_dir.join("ghostcoded.pid"),
            log: effective_daemon_dir.join("ghostcoded.log"),
            daemon_dir: effective_daemon_dir,
        }
    }
}

/// 简单的字符串哈希（用于生成 /tmp 后备目录名）
fn simple_hash(s: &str) -> String {
    let mut hash: u64 = 5381;
    for byte in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    format!("{:016x}", hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_path_no_fallback() {
        let paths = DaemonPaths::new(Path::new("/Users/test/.ghostcode"));
        assert!(paths.sock.to_string_lossy().contains(".ghostcode"));
    }

    #[test]
    fn long_path_uses_tmp_fallback() {
        // 构造一个超长路径
        let long_base = format!("/Users/{}/very/deeply/nested/directory/structure", "a".repeat(80));
        let paths = DaemonPaths::new(Path::new(&long_base));
        assert!(
            paths.sock.to_string_lossy().starts_with("/tmp/ghostcode-"),
            "超长路径应使用 /tmp 后备: {}",
            paths.sock.display()
        );
    }
}
