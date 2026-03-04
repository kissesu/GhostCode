//! GhostCode MCP Server 可执行入口
//!
//! 作为独立二进制供 Claude Code 通过 .mcp.json 启动
//! 实现 stdio JSON-RPC 2.0 桥接
//!
//! 冷启动自举流程：
//! 1. 尝试读取 addr.json 获取 Daemon 地址
//! 2. 若 addr.json 不存在，自动启动 Daemon（后台 spawn ghostcoded）
//! 3. 轮询等待 addr.json 出现（最多 5 秒，200ms 间隔）
//! 4. 连接 Daemon，启动 stdio JSON-RPC 2.0 循环
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

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;

/// Daemon 自举等待超时时间（毫秒）
const DAEMON_BOOT_TIMEOUT_MS: u64 = 5000;

/// Daemon 自举轮询间隔（毫秒）
const DAEMON_POLL_INTERVAL_MS: u64 = 200;

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

/// 尝试自动启动 Daemon 守护进程
///
/// 业务逻辑：
/// 1. 在 base_dir/bin/ 下查找 ghostcoded 二进制
/// 2. 以后台进程方式 spawn（detached，stdin/stdout/stderr 置空）
/// 3. 不等待进程结束，立即返回
///
/// @param base_dir - GhostCode 基准目录（如 ~/.ghostcode/）
/// @return 启动成功返回 Ok，找不到二进制或 spawn 失败返回 Err
fn try_spawn_daemon(base_dir: &Path) -> Result<()> {
    let daemon_bin = base_dir.join("bin").join("ghostcoded");

    if !daemon_bin.exists() {
        anyhow::bail!(
            "Daemon 二进制不存在: {}，请运行 ghostcode init 安装",
            daemon_bin.display()
        );
    }

    // 以后台进程方式启动 Daemon
    // --base-dir 传入当前基准目录，确保路径一致
    std::process::Command::new(&daemon_bin)
        .arg("--base-dir")
        .arg(base_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("启动 Daemon 失败: {}", daemon_bin.display()))?;

    Ok(())
}

/// 等待 Daemon addr.json 出现并解析地址
///
/// 业务逻辑：
/// 1. 以 DAEMON_POLL_INTERVAL_MS 间隔轮询 addr.json
/// 2. 超过 DAEMON_BOOT_TIMEOUT_MS 则超时报错
/// 3. addr.json 出现后解析并返回 socket 路径
///
/// @param base_dir - GhostCode 基准目录
/// @return 解析出的 Daemon socket 路径
async fn wait_for_daemon_addr(base_dir: &Path) -> Result<PathBuf> {
    let poll_interval = Duration::from_millis(DAEMON_POLL_INTERVAL_MS);
    let timeout = Duration::from_millis(DAEMON_BOOT_TIMEOUT_MS);
    let start = std::time::Instant::now();

    loop {
        // 尝试解析 addr.json
        if let Ok(addr) = ghostcode_mcp::bootstrap::resolve_daemon_addr(base_dir) {
            return Ok(addr);
        }

        // 检查是否超时
        if start.elapsed() > timeout {
            anyhow::bail!(
                "等待 Daemon 启动超时（{}ms），addr.json 未出现。\n\
                 请检查 Daemon 是否能正常启动：ghostcoded --base-dir {}",
                DAEMON_BOOT_TIMEOUT_MS,
                base_dir.display()
            );
        }

        tokio::time::sleep(poll_interval).await;
    }
}

/// 程序入口
///
/// 业务逻辑（含冷启动自举）：
/// 1. 解析命令行参数（--base-dir 可选）
/// 2. 确定基准目录（CLI 参数 > 默认 $HOME/.ghostcode/）
/// 3. 尝试从 addr.json 解析 Daemon 地址
/// 4. 若 addr.json 不存在，自动 spawn Daemon 并轮询等待就绪
/// 5. 启动 stdio JSON-RPC 2.0 服务器循环
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
    // 第三步：解析 Daemon 地址（含冷启动自举）
    //
    // 先尝试直接读取 addr.json（热路径，Daemon 已运行时零延迟）
    // 若不存在，自动启动 Daemon 并等待 addr.json 出现
    // ============================================
    let daemon_addr = match ghostcode_mcp::bootstrap::resolve_daemon_addr(&base_dir) {
        Ok(addr) => addr,
        Err(_) => {
            // addr.json 不存在，尝试自动启动 Daemon
            eprintln!("[GhostCode MCP] Daemon 未运行，尝试自动启动...");

            if let Err(e) = try_spawn_daemon(&base_dir) {
                eprintln!("[GhostCode MCP] 自动启动 Daemon 失败: {}", e);
                // 启动失败仍然等待一小段时间，可能有其他进程正在启动 Daemon
            }

            // 轮询等待 Daemon 就绪
            wait_for_daemon_addr(&base_dir)
                .await
                .context("Daemon 自举失败，请手动运行 ghostcode init 或 ghostcoded")?
        }
    };

    // ============================================
    // 第四步：启动 stdio MCP 服务器
    // group_id 和 actor_id 由 initialize 握手后更新（当前用占位值）
    // ============================================
    ghostcode_mcp::serve_stdio("default", "mcp", &daemon_addr).await?;

    Ok(())
}
