//! Web 服务器应用状态
//!
//! 持有账本基础路径，供 handler 构造 group 账本路径
//!
//! @author Atlas.oi
//! @date 2026-03-03

use std::path::PathBuf;

/// Web 服务器应用状态
#[derive(Debug, Clone)]
pub struct WebState {
    /// GhostCode 数据根目录（默认 ~/.ghostcode）
    pub data_root: PathBuf,
}

impl WebState {
    /// 创建新的 WebState
    ///
    /// @param data_root - GhostCode 数据根目录
    pub fn new(data_root: PathBuf) -> Self {
        Self { data_root }
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
