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
    /// daemon_socket_path 默认为 {data_root}/daemon/ghostcoded.sock
    /// 与 ghostcode-daemon 的 DaemonPaths 保持一致
    ///
    /// @param data_root - GhostCode 数据根目录
    pub fn new(data_root: PathBuf) -> Self {
        // 默认 socket 路径：{data_root}/daemon/ghostcoded.sock
        // 注意：与 ghostcode-daemon/src/paths.rs 中 DaemonPaths::new() 保持一致
        let daemon_socket_path = data_root.join("daemon").join("ghostcoded.sock");
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

    /// 校验 group_id 是否为合法格式
    ///
    /// 安全约束：仅允许字母、数字、连字符、下划线
    /// 拒绝包含路径分隔符（/ \）或目录遍历（..）的输入
    /// 防止路径穿越攻击（如 ../../etc/passwd）
    ///
    /// @param group_id - 待校验的 Group ID
    /// @returns 合法返回 true，非法返回 false
    pub fn is_valid_group_id(group_id: &str) -> bool {
        !group_id.is_empty()
            && group_id
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    }

    /// 构造指定 Group 的账本文件路径
    ///
    /// 格式: {data_root}/groups/{group_id}/state/ledger/ledger.jsonl
    /// 与 ghostcode-daemon 的写入路径保持一致
    ///
    /// 安全：group_id 必须通过 is_valid_group_id 校验，
    /// 非法输入返回 None，防止路径穿越攻击
    ///
    /// @param group_id - Group ID（仅允许字母、数字、连字符、下划线）
    /// @returns 合法时返回 Some(路径)，非法输入返回 None
    pub fn ledger_path(&self, group_id: &str) -> Option<PathBuf> {
        if !Self::is_valid_group_id(group_id) {
            return None;
        }
        Some(
            self.data_root
                .join("groups")
                .join(group_id)
                .join("state")
                .join("ledger")
                .join("ledger.jsonl"),
        )
    }

    /// 构造默认的 WebState（使用 ~/.ghostcode 作为数据根目录）
    pub fn default_state() -> Self {
        let data_root = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".ghostcode");
        Self::new(data_root)
    }
}
