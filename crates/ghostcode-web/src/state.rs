//! Web 服务器应用状态
//!
//! 持有账本基础路径和 daemon Unix Socket 路径
//! 供 handler 构造 group 账本路径以及 IPC client 连接 daemon
//!
//! @author Atlas.oi
//! @date 2026-03-04

use std::path::PathBuf;

/// Web 服务器应用状态
#[derive(Debug, Clone)]
pub struct WebState {
    /// GhostCode 数据根目录（默认 ~/.ghostcode）
    pub data_root: PathBuf,
    /// daemon Unix Socket 路径，供 IPC client 连接
    /// 默认为 ~/.ghostcode/daemon.sock
    pub daemon_socket_path: PathBuf,
}

impl WebState {
    /// 创建新的 WebState（使用默认 daemon socket 路径）
    ///
    /// daemon_socket_path 默认为 {data_root}/daemon.sock
    ///
    /// @param data_root - GhostCode 数据根目录
    pub fn new(data_root: PathBuf) -> Self {
        // 默认 socket 路径：{data_root}/daemon.sock
        let daemon_socket_path = data_root.join("daemon.sock");
        Self {
            data_root,
            daemon_socket_path,
        }
    }

    /// 创建新的 WebState 并显式指定 daemon socket 路径
    ///
    /// 用于测试或需要自定义 socket 路径的场景
    ///
    /// @param data_root - GhostCode 数据根目录
    /// @param daemon_socket_path - daemon Unix Socket 文件路径
    pub fn with_socket(data_root: PathBuf, daemon_socket_path: PathBuf) -> Self {
        Self {
            data_root,
            daemon_socket_path,
        }
    }

    /// 构造指定 Group 的账本文件路径
    ///
    /// 格式: {data_root}/groups/{group_id}/ledger.ndjson
    ///
    /// @param group_id - Group ID
    /// @returns 账本文件路径
    pub fn ledger_path(&self, group_id: &str) -> PathBuf {
        self.data_root
            .join("groups")
            .join(group_id)
            .join("ledger.ndjson")
    }

    /// 构造默认的 WebState（使用 ~/.ghostcode 作为数据根目录）
    pub fn default_state() -> Self {
        let data_root = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".ghostcode");
        Self::new(data_root)
    }
}
