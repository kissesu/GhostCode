//! JSON-RPC 2.0 数据结构定义
//!
//! 定义 MCP stdio 传输层使用的请求/响应/错误类型
//! 参考: cccc/src/cccc/ports/mcp/main.py:93-103 - _make_response / _make_error
//!
//! @author Atlas.oi
//! @date 2026-03-01

use serde::{Deserialize, Serialize};

/// JSON-RPC 2.0 请求
///
/// 从 stdin 逐行读取并反序列化
/// id 为 serde_json::Value，支持 Number / String / Null
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcRequest {
    /// 协议版本，固定为 "2.0"
    pub jsonrpc: String,
    /// 请求 ID，Number / String / Null
    pub id: serde_json::Value,
    /// 方法名
    pub method: String,
    /// 方法参数，可为 object 或 array
    #[serde(default)]
    pub params: serde_json::Value,
}

/// JSON-RPC 2.0 响应
///
/// 序列化后写入 stdout，末尾加 "\n" 并 flush
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse {
    /// 协议版本，固定为 "2.0"
    pub jsonrpc: String,
    /// 与请求对应的 ID
    pub id: serde_json::Value,
    /// 成功时的结果（与 error 互斥）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// 失败时的错误对象（与 result 互斥）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 错误对象
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcError {
    /// 错误码（参见 JSON-RPC 2.0 规范）
    /// -32601: Method not found
    /// -32600: Invalid Request
    /// -32603: Internal error
    pub code: i32,
    /// 可读错误描述
    pub message: String,
    /// 附加错误数据（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcResponse {
    /// 构造成功响应
    ///
    /// # 参数
    /// - id: 对应请求的 ID，原样返回
    /// - result: 任意 JSON 值
    pub fn ok(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// 构造错误响应
    ///
    /// # 参数
    /// - id: 对应请求的 ID，原样返回
    /// - code: 错误码（如 -32601）
    /// - message: 可读错误描述
    pub fn error(id: serde_json::Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // proptest: response_id_matches_request
    // 任意 i64 id，JsonRpcResponse::ok(id, result).id 必须与输入 id 一致
    proptest! {
        #[test]
        fn response_id_matches_request(n in any::<i64>()) {
            let id = serde_json::json!(n);
            let resp = JsonRpcResponse::ok(id.clone(), serde_json::json!({}));
            prop_assert_eq!(resp.id, id, "响应 id 必须与请求 id 一致");
            prop_assert_eq!(resp.jsonrpc, "2.0", "jsonrpc 字段必须为 '2.0'");
        }
    }

    #[test]
    fn ok_response_has_no_error_field() {
        let resp = JsonRpcResponse::ok(serde_json::json!(1), serde_json::json!({}));
        assert!(resp.error.is_none(), "成功响应不应含 error 字段");

        // 验证序列化后不含 error 键
        let json_str = serde_json::to_string(&resp).unwrap();
        assert!(!json_str.contains("\"error\""), "序列化后不应含 error 字段");
    }

    #[test]
    fn error_response_has_no_result_field() {
        let resp = JsonRpcResponse::error(serde_json::json!(1), -32601, "Method not found");
        assert!(resp.result.is_none(), "错误响应不应含 result 字段");

        // 验证序列化后不含 result 键
        let json_str = serde_json::to_string(&resp).unwrap();
        assert!(!json_str.contains("\"result\""), "序列化后不应含 result 字段");
    }
}
