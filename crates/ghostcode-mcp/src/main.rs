//! GhostCode MCP Server 可执行入口
//!
//! 作为独立二进制供 Claude Code 通过 .mcp.json 启动
//! 实现 stdio JSON-RPC 2.0 桥接
//!
//! 启动方式（.mcp.json 配置示例）：
//! ```json
//! {
//!   "mcpServers": {
//!     "ghostcode": {
//!       "command": "ghostcode-mcp",
//!       "args": ["--base-dir", "/path/to/project"]
//!     }
//!   }
//! }
//! ```
//!
//! @author Atlas.oi
//! @date 2026-03-04

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

/// GhostCode MCP Server 命令行参数
///
/// 通过 clap derive 宏自动生成参数解析逻辑
#[derive(Parser, Debug)]
#[command(
    name = "ghostcode-mcp",
    about = "GhostCode MCP Server - 为 Claude Code 提供多 Agent 协作工具",
    version
)]
struct Args {
    /// 项目基准目录，Daemon socket 和数据文件的根路径
    /// 默认使用 $HOME/.ghostcode/
    #[arg(long, value_name = "DIR")]
    base_dir: Option<PathBuf>,
}

/// 程序入口
///
/// 业务逻辑：
/// 1. 解析命令行参数（--base-dir 可选）
/// 2. 确定基准目录（CLI 参数 > 默认 $HOME/.ghostcode/）
/// 3. 从 addr.json 动态解析 Daemon Socket 地址
/// 4. 启动 stdio JSON-RPC 2.0 服务器循环
#[tokio::main]
async fn main() -> Result<()> {
    // ============================================
    // 第一步：解析命令行参数
    // ============================================
    let args = Args::parse();

    // ============================================
    // 第二步：确定基准目录
    // 优先使用 --base-dir 参数，否则使用默认路径 $HOME/.ghostcode/
    // ============================================
    let base_dir = args
        .base_dir
        .unwrap_or_else(ghostcode_mcp::bootstrap::default_base_dir);

    // ============================================
    // 第三步：从 addr.json 动态解析 Daemon Socket 地址
    // Daemon 启动时会写入 addr.json，包含实际的 socket 路径
    // 若 addr.json 不存在，说明 Daemon 未启动，给出明确错误提示
    // ============================================
    let daemon_addr = ghostcode_mcp::bootstrap::resolve_daemon_addr(&base_dir)
        .context("无法解析 Daemon 地址，请确保 GhostCode Daemon 已启动")?;

    // ============================================
    // 第四步：启动 stdio MCP 服务器
    // group_id 和 actor_id 由 initialize 握手后更新（当前用占位值）
    // ============================================
    ghostcode_mcp::serve_stdio("default", "mcp", &daemon_addr).await?;

    Ok(())
}
