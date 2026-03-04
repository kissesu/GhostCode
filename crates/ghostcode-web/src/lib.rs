//! ghostcode-web: GhostCode Web Dashboard HTTP 服务器
//!
//! 提供 Dashboard 数据的 REST API 和 SSE 实时流接口
//! 独立于 ghostcode-daemon 运行，通过账本文件共享数据
//! 通过 Unix Socket IPC client 与 ghostcode-daemon 通信（skills 相关操作）
//!
//! @author Atlas.oi
//! @date 2026-03-04

pub mod dto;
pub mod handlers;
pub mod ipc;
pub mod server;
pub mod sse;
pub mod state;
