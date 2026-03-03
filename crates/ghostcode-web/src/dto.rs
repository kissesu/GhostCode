//! Dashboard HTTP 响应 DTO
//!
//! 将内部 ghostcode-types 类型包装为统一的 HTTP 响应格式
//!
//! @author Atlas.oi
//! @date 2026-03-03

use serde::Serialize;

/// 统一 API 响应包装
///
/// 所有 REST 端点返回此格式，方便前端统一处理
#[derive(Serialize)]
pub struct ApiResponse<T: Serialize> {
    /// 是否成功
    pub ok: bool,
    /// 响应数据
    pub data: T,
}

impl<T: Serialize> ApiResponse<T> {
    /// 构造成功响应
    pub fn ok(data: T) -> Self {
        Self { ok: true, data }
    }
}
