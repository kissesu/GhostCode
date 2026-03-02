//! 进程管理
//!
//! 端点描述符读写、PID 文件管理、残留文件清理
//! 参考: cccc/src/cccc/daemon_main.py + daemon/server.py:375-434
//!
//! @author Atlas.oi
//! @date 2026-03-01

use std::fs;
use std::path::Path;

use ghostcode_types::addr::AddrDescriptor;

/// 进程管理错误类型
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ProcessError>;

/// 写入端点描述符
///
/// 将 AddrDescriptor 序列化为 JSON 写入文件
/// 客户端通过读取此文件获取 Daemon 连接信息
///
/// @param addr_path - 端点描述符文件路径（如 ghostcoded.addr.json）
/// @param descriptor - 端点描述符
pub fn write_addr_descriptor(addr_path: &Path, descriptor: &AddrDescriptor) -> Result<()> {
    // 确保父目录存在
    if let Some(parent) = addr_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(descriptor)?;
    fs::write(addr_path, json)?;
    Ok(())
}

/// 读取端点描述符（CLI 端使用）
///
/// 文件不存在返回 None（非错误，表示 Daemon 未运行）
///
/// @param addr_path - 端点描述符文件路径
/// @return Some(descriptor) 或 None（文件不存在时）
pub fn read_addr_descriptor(addr_path: &Path) -> Result<Option<AddrDescriptor>> {
    if !addr_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(addr_path)?;
    let descriptor: AddrDescriptor = serde_json::from_str(&content)?;
    Ok(Some(descriptor))
}

/// 写入 PID 文件
///
/// @param pid_path - PID 文件路径
/// @param pid - 进程 ID
pub fn write_pid_file(pid_path: &Path, pid: u32) -> Result<()> {
    if let Some(parent) = pid_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(pid_path, pid.to_string())?;
    Ok(())
}

/// 清理残留文件（socket + addr + pid）
///
/// Daemon 启动时调用，清除上次异常退出遗留的文件
/// 仅清理特定的 daemon 文件，不删除目录本身
///
/// @param daemon_dir - daemon 目录路径
pub fn cleanup_stale_files(daemon_dir: &Path) -> Result<()> {
    let stale_files = [
        "ghostcoded.sock",
        "ghostcoded.addr.json",
        "ghostcoded.pid",
    ];

    for filename in &stale_files {
        let path = daemon_dir.join(filename);
        if path.exists() {
            fs::remove_file(&path)?;
        }
    }

    Ok(())
}
