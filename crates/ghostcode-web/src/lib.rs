//! ghostcode-web: GhostCode Web Dashboard HTTP 服务器
//!
//! 提供 Dashboard 数据的 REST API 和 SSE 实时流接口
//! 独立于 ghostcode-daemon 运行，通过账本文件共享数据
//!
//! @author Atlas.oi
//! @date 2026-03-03

pub mod dto;
pub mod handlers;
pub mod server;
pub mod sse;
pub mod state;
