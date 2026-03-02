//! 端点描述符类型定义
//!
//! AddrDescriptor
//! Daemon 启动后写入的端点信息，供客户端连接时读取
//! 参考: cccc/src/cccc/daemon/server.py:375-434
//!
//! @author Atlas.oi
//! @date 2026-02-28

use serde::{Deserialize, Serialize};

/// 端点描述符
///
/// Daemon 启动后写入到 addr 文件中
/// 客户端通过读取此描述符获取连接信息
///
/// 字段说明：
/// - v: 协议版本号，固定为 1
/// - transport: 传输协议（目前固定为 "unix"）
/// - path: Unix Socket 文件路径
/// - pid: Daemon 进程 ID
/// - version: GhostCode 版本号
/// - ts: 启动时间 ISO 8601 UTC
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddrDescriptor {
    /// 协议版本号，固定为 1
    pub v: u8,
    /// 传输协议（"unix"）
    pub transport: String,
    /// Unix Socket 文件路径
    pub path: String,
    /// Daemon 进程 ID
    pub pid: u32,
    /// GhostCode 版本号
    pub version: String,
    /// 启动时间 ISO 8601 UTC
    pub ts: String,
}

impl AddrDescriptor {
    /// 创建新的端点描述符
    pub fn new(path: impl Into<String>, pid: u32, version: impl Into<String>) -> Self {
        Self {
            v: 1,
            transport: "unix".to_string(),
            path: path.into(),
            pid,
            version: version.into(),
            ts: chrono::Utc::now()
                .to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        }
    }
}
