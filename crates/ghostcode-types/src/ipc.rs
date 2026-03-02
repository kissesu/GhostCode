//! IPC 协议类型定义
//!
//! DaemonRequest / DaemonResponse / DaemonError
//! Rust Daemon 与 TS Plugin 之间的 JSON-RPC 通信协议
//!
//! @author Atlas.oi
//! @date 2026-02-28

use serde::{Deserialize, Serialize};

/// Daemon 请求
///
/// TS Plugin 发送给 Rust Daemon 的操作请求
/// 通过 Unix Socket 传输
///
/// 字段说明：
/// - v: 协议版本号，固定为 1
/// - op: 操作名称（如 "ping", "actor.add", "chat.send"）
/// - args: 操作参数，任意 JSON 值
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DaemonRequest {
    /// 协议版本号，固定为 1
    pub v: u8,
    /// 操作名称
    pub op: String,
    /// 操作参数
    pub args: serde_json::Value,
}

impl DaemonRequest {
    /// 创建新请求
    pub fn new(op: impl Into<String>, args: serde_json::Value) -> Self {
        Self {
            v: 1,
            op: op.into(),
            args,
        }
    }
}

/// Daemon 响应
///
/// Rust Daemon 返回给 TS Plugin 的操作结果
///
/// 字段说明：
/// - v: 协议版本号，固定为 1
/// - ok: 操作是否成功
/// - result: 成功时的返回值
/// - error: 失败时的错误信息
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DaemonResponse {
    /// 协议版本号，固定为 1
    pub v: u8,
    /// 操作是否成功
    pub ok: bool,
    /// 成功时的返回值
    pub result: serde_json::Value,
    /// 失败时的错误信息
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<DaemonError>,
}

impl DaemonResponse {
    /// 创建成功响应
    pub fn ok(result: serde_json::Value) -> Self {
        Self {
            v: 1,
            ok: true,
            result,
            error: None,
        }
    }

    /// 创建错误响应
    pub fn err(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            v: 1,
            ok: false,
            result: serde_json::Value::Null,
            error: Some(DaemonError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

/// Daemon 错误信息
///
/// 包含错误码和可读的错误描述
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DaemonError {
    /// 错误码（如 "NOT_FOUND", "INVALID_ARGS"）
    pub code: String,
    /// 可读的错误描述
    pub message: String,
}
