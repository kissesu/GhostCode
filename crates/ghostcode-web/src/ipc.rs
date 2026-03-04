/**
 * @file ipc.rs
 * @description Unix Socket IPC client，用于 ghostcode-web 连接 ghostcode-daemon
 *
 * 实现向 daemon 发送 DaemonRequest 并接收 DaemonResponse 的客户端逻辑
 * 连接失败时返回可区分的 IpcError，供上层 handler 映射为 HTTP 502
 *
 * 协议说明：
 * 1. 建立 Unix Socket 连接
 * 2. 发送 JSON 序列化的 DaemonRequest，以换行符结束（NDJSON 格式）
 * 3. 读取一行 JSON 响应，反序列化为 DaemonResponse
 * 4. 连接关闭
 *
 * @author Atlas.oi
 * @date 2026-03-04
 */

use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use ghostcode_types::ipc::{DaemonRequest, DaemonResponse};

/// IPC 调用错误类型
///
/// 区分连接失败（daemon 不可达）和协议错误（序列化/反序列化失败）
#[derive(Debug)]
pub enum IpcError {
    /// daemon 不可达：socket 文件不存在或连接被拒绝
    ConnectionFailed(std::io::Error),
    /// 序列化请求失败（不应发生，属于编程错误）
    SerializeFailed(serde_json::Error),
    /// 读取/反序列化响应失败
    DeserializeFailed(String),
    /// IO 读写错误
    IoError(std::io::Error),
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpcError::ConnectionFailed(e) => write!(f, "daemon 连接失败: {}", e),
            IpcError::SerializeFailed(e) => write!(f, "请求序列化失败: {}", e),
            IpcError::DeserializeFailed(s) => write!(f, "响应反序列化失败: {}", s),
            IpcError::IoError(e) => write!(f, "IPC IO 错误: {}", e),
        }
    }
}

/// 通过 Unix Socket 向 daemon 发送请求并获取响应
///
/// 业务逻辑：
/// 1. 连接到 daemon 的 Unix Socket（失败 -> IpcError::ConnectionFailed）
/// 2. 序列化 DaemonRequest 为 JSON，写入 socket（以换行符结束）
/// 3. 从 socket 读取一行 JSON 响应
/// 4. 反序列化为 DaemonResponse 并返回
///
/// @param socket_path - daemon Unix Socket 文件路径
/// @param request - 要发送的 DaemonRequest
/// @returns DaemonResponse 或 IpcError
pub async fn call_daemon(
    socket_path: &Path,
    request: &DaemonRequest,
) -> Result<DaemonResponse, IpcError> {
    // ============================================
    // 第一步：连接到 daemon Unix Socket
    // 连接失败说明 daemon 未启动或 socket 路径不存在
    // ============================================
    let stream = UnixStream::connect(socket_path)
        .await
        .map_err(IpcError::ConnectionFailed)?;

    let (reader_half, mut writer_half) = stream.into_split();

    // ============================================
    // 第二步：序列化并发送请求
    // DaemonRequest 序列化为 NDJSON 行格式（以 \n 结束）
    // ============================================
    let req_json = serde_json::to_string(request)
        .map_err(IpcError::SerializeFailed)?;
    let req_line = req_json + "\n";

    writer_half
        .write_all(req_line.as_bytes())
        .await
        .map_err(IpcError::IoError)?;

    // ============================================
    // 第三步：读取并反序列化响应
    // 读取一行 JSON 响应（以 \n 结束）
    // ============================================
    let mut reader = BufReader::new(reader_half);
    let mut resp_line = String::new();
    reader
        .read_line(&mut resp_line)
        .await
        .map_err(IpcError::IoError)?;

    if resp_line.is_empty() {
        return Err(IpcError::DeserializeFailed(
            "daemon 返回了空响应".to_string(),
        ));
    }

    let response: DaemonResponse = serde_json::from_str(resp_line.trim())
        .map_err(|e| IpcError::DeserializeFailed(format!("JSON 解析失败: {e}, 原始: {resp_line}")))?;

    Ok(response)
}
