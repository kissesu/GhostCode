//! bootstrap - Daemon 地址解析与自举
//!
//! 从 addr.json 文件中读取 Daemon 的 Unix Socket 路径。
//! 当 addr.json 不存在时返回错误，由调用方决定是否启动 Daemon。
//!
//! addr.json 格式示例：
//! ```json
//! {
//!   "v": 1,
//!   "transport": "unix",
//!   "path": "/Users/xxx/.ghostcode/daemon/ghostcoded.sock",
//!   "pid": 12345,
//!   "version": "0.1.0",
//!   "ts": "2026-03-04T04:00:00Z"
//! }
//! ```
//!
//! @author Atlas.oi
//! @date 2026-03-04

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// 从 base_dir/daemon/ghostcoded.addr.json 解析 Daemon Socket 路径
///
/// 读取 addr.json 并提取 "path" 字段作为 Unix Socket 路径。
///
/// 业务逻辑：
/// 1. 构建 addr.json 文件路径：base_dir/daemon/ghostcoded.addr.json
/// 2. 读取文件内容，不存在时返回包含文件路径的错误消息
/// 3. 解析 JSON，格式无效时返回 "addr.json 格式无效" 错误
/// 4. 提取 "path" 字段，缺少时返回 "addr.json 缺少 path 字段" 错误
///
/// @param base_dir - GhostCode 基准目录（如 ~/.ghostcode/）
/// @return Socket 文件的 PathBuf
/// @throws anyhow::Error - addr.json 不存在、格式无效或缺少 path 字段时抛出
pub fn resolve_daemon_addr(base_dir: &Path) -> Result<PathBuf> {
    let addr_path = base_dir.join("daemon").join("ghostcoded.addr.json");

    // 读取 addr.json 文件，路径不存在时给出明确错误消息
    let content = std::fs::read_to_string(&addr_path)
        .with_context(|| format!("无法读取 addr.json: {}", addr_path.display()))?;

    // 解析 JSON 内容
    let parsed: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| "addr.json 格式无效")?;

    // 提取 "path" 字段作为 socket 路径
    let sock_path = parsed
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("addr.json 缺少 path 字段"))?;

    Ok(PathBuf::from(sock_path))
}

/// 获取默认的 GhostCode 基准目录
///
/// 优先使用 HOME 环境变量，其次尝试 USERPROFILE（Windows 兼容），
/// 均不存在时回退到 /tmp。
///
/// 默认目录为 $HOME/.ghostcode/
///
/// @return GhostCode 基准目录的 PathBuf
pub fn default_base_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".to_string());

    PathBuf::from(home).join(".ghostcode")
}
