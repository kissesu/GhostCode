//! GhostCode MCP Server
//!
//! 将 GhostCode 功能暴露为 MCP 工具供 Claude Code 调用
//! 实现 stdio JSON-RPC 2.0 服务器协议，包含 8 个核心工具
//!
//! @author Atlas.oi
//! @date 2026-03-01

pub mod jsonrpc;
pub mod server;
pub mod tools;

// 重新导出公共 API
pub use server::serve_stdio;
pub use tools::{ToolContext, ToolError};
