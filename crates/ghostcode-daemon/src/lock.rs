//! 单实例锁管理
//!
//! 使用 fs2 的非阻塞排他锁确保同一时间只有一个 Daemon 实例运行
//! 参考: cccc/src/cccc/util/file_lock.py
//!
//! @author Atlas.oi
//! @date 2026-03-01

use std::fs::{self, File, OpenOptions};
use std::path::Path;

use fs2::FileExt;

/// 锁错误类型
#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("Daemon 已在运行（锁已被占用）")]
    AlreadyRunning,
}

pub type Result<T> = std::result::Result<T, LockError>;

/// 尝试获取单实例锁（非阻塞）
///
/// 成功返回锁文件句柄（持有期间锁不释放）
/// 失败说明有另一个 Daemon 实例在运行
///
/// 业务逻辑：
/// 1. 确保锁文件父目录存在
/// 2. 打开或创建锁文件
/// 3. 尝试非阻塞获取排他锁
/// 4. 成功：返回 File 句柄（调用者需持有）
/// 5. 失败：返回 AlreadyRunning 错误
///
/// @param lock_path - 锁文件路径（如 ~/.ghostcode/daemon/ghostcoded.lock）
/// @return 锁文件句柄（持有期间锁不释放，drop 时自动释放）
pub fn try_acquire_singleton_lock(lock_path: &Path) -> Result<File> {
    // 确保父目录存在
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_path)?;

    // 非阻塞尝试获取排他锁
    match lock_file.try_lock_exclusive() {
        Ok(()) => Ok(lock_file),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Err(LockError::AlreadyRunning),
        Err(e) => Err(LockError::Io(e)),
    }
}
