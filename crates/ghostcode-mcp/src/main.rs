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

/// 从 groups 目录自动检测当前活跃的 group_id
///
/// 扫描 base_dir/groups/ 下以 "g-" 开头的目录，
/// 按 ledger 文件修改时间排序，返回最近活跃的 group
/// 与 handle_active_group（Web 端点）逻辑保持一致
///
/// @param base_dir - GhostCode 基准目录（如 ~/.ghostcode/）
/// @return 检测到的 group_id，或 None
fn detect_group_id(base_dir: &Path) -> Option<String> {
    let groups_dir = base_dir.join("groups");
    let mut best: Option<(String, std::time::SystemTime)> = None;

    for entry in std::fs::read_dir(&groups_dir).ok()?.flatten() {
        // 确保文件名是合法 UTF-8，非 UTF-8 名称直接跳过
        let name_str = match entry.file_name().to_str() {
            Some(s) => s.to_string(),
            None => continue,
        };
        if !name_str.starts_with("g-") {
            continue;
        }
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        // 按 ledger 文件修改时间排序，选择最近活跃的 group
        let ledger = base_dir
            .join("groups")
            .join(&name_str)
            .join("state")
            .join("ledger")
            .join("ledger.jsonl");
        if let Ok(meta) = std::fs::metadata(&ledger) {
            if let Ok(modified) = meta.modified() {
                match &best {
                    Some((_, prev_time)) if modified > *prev_time => {
                        best = Some((name_str, modified));
                    }
                    None => {
                        best = Some((name_str, modified));
                    }
                    _ => {}
                }
            }
        }
    }

    best.map(|(id, _)| id)
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
    // 第四步：确定 group_id 和 actor_id（不使用 unsafe set_var）
    //
    // MCP 工具通过参数传递 group_id 和 actor_id，而非全局环境变量
    // 避免在多线程 tokio runtime 中调用 unsafe env::set_var 导致数据竞争（UB）
    //
    // 优先级：环境变量（外部注入） > 自动检测 > 默认值
    // ============================================
    let group_id = {
        let from_env = std::env::var("GHOSTCODE_GROUP_ID").unwrap_or_default();
        if !from_env.is_empty() {
            from_env
        } else if let Some(gid) = detect_group_id(&base_dir) {
            eprintln!("[GhostCode MCP] 自动检测 group_id: {}", gid);
            gid
        } else {
            "default".to_string()
        }
    };

    let actor_id = {
        let from_env = std::env::var("GHOSTCODE_ACTOR_ID").unwrap_or_default();
        if !from_env.is_empty() {
            from_env
        } else {
            eprintln!("[GhostCode MCP] 使用默认 actor_id: claude-main");
            "claude-main".to_string()
        }
    };

    // ============================================
    // 第五步：启动 stdio MCP 服务器
    // ============================================
    ghostcode_mcp::serve_stdio(&group_id, &actor_id, &daemon_addr).await?;

    Ok(())
}
