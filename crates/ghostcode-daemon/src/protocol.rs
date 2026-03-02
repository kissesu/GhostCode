//! JSON-RPC 协议层
//!
//! 负责从 Unix Socket 流中读取 DaemonRequest 和写入 DaemonResponse
//! 协议规范：每个请求/响应是一行 JSON + \n
//!
//! 参考: cccc/src/cccc/daemon/socket_protocol_ops.py
//!
//! @author Atlas.oi
//! @date 2026-03-01

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

use ghostcode_types::ipc::{DaemonRequest, DaemonResponse};

/// 单个请求的最大字节数：2MB [ERR-2]
/// 超过此限制的请求将被拒绝，防止恶意大请求耗尽内存
const MAX_REQUEST_SIZE: usize = 2 * 1024 * 1024;

/// 协议错误类型
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),

    #[error("请求超过大小限制 ({0} bytes > {1} bytes)")]
    OversizedRequest(usize, usize),

    #[error("连接已关闭")]
    ConnectionClosed,
}

pub type Result<T> = std::result::Result<T, ProtocolError>;

/// 从流中读取一个 DaemonRequest
///
/// 读取一行 JSON 并反序列化为 DaemonRequest
/// 空行（连接关闭）返回 None
/// 超过 2MB [ERR-2] 返回 OversizedRequest 错误
///
/// @param reader - 带缓冲的读取半部
/// @return Some(request) 或 None（连接关闭时）
pub async fn read_request(
    reader: &mut BufReader<OwnedReadHalf>,
) -> Result<Option<DaemonRequest>> {
    let mut line = String::new();

    let bytes_read = reader.read_line(&mut line).await?;
    if bytes_read == 0 {
        return Ok(None);
    }

    // [ERR-2] 检查请求大小
    if line.len() > MAX_REQUEST_SIZE {
        return Err(ProtocolError::OversizedRequest(line.len(), MAX_REQUEST_SIZE));
    }

    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let request: DaemonRequest = serde_json::from_str(trimmed)?;
    Ok(Some(request))
}

/// 写入一个 DaemonResponse
///
/// 将 DaemonResponse 序列化为一行 JSON + \n 写入流
///
/// @param writer - 写入半部
/// @param response - 要发送的响应
pub async fn write_response(
    writer: &mut OwnedWriteHalf,
    response: &DaemonResponse,
) -> Result<()> {
    let json = serde_json::to_string(response)?;
    let line = format!("{}\n", json);
    writer.write_all(line.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}
