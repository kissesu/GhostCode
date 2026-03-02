//! GhostCode 常驻守护进程入口
//!
//! 负责 Agent 生命周期管理、消息路由、Unix Socket IPC 通信
//! 参考: cccc/src/cccc/daemon_main.py - 单写者进程 + Unix Socket IPC
//!
//! 启动流程：
//! clap 参数解析 -> 构建 StartupConfig -> run_daemon（完整启动链路）
//!
//! @author Atlas.oi
//! @date 2026-03-02

use std::path::PathBuf;

use clap::Parser;

use ghostcode_daemon::startup::{StartupConfig, run_daemon};

/// GhostCode Daemon 命令行参数
///
/// 通过 clap derive 宏自动生成 CLI 解析逻辑
#[derive(Parser, Debug)]
#[command(
    name = "ghostcoded",
    version = env!("CARGO_PKG_VERSION"),
    about = "GhostCode 常驻守护进程 - 多 Agent 协作开发平台核心"
)]
struct Args {
    /// 基础目录（默认: ~/.ghostcode/）
    ///
    /// 所有 Daemon 文件（socket、addr.json、PID、log）将存储于此目录下的 daemon/ 子目录
    #[arg(long, default_value_os_t = default_base_dir())]
    base_dir: PathBuf,

    /// groups 根目录路径（默认: ~/.ghostcode/groups/）
    ///
    /// 存储 Agent 分组配置的根目录，每个 group 是一个子目录
    #[arg(long)]
    groups_dir: Option<PathBuf>,
}

/// 获取默认基础目录（~/.ghostcode/）
fn default_base_dir() -> PathBuf {
    // 优先使用 HOME 环境变量，回退到 /tmp/ghostcode
    dirs_or_home()
}

/// 获取用户主目录下的 .ghostcode 目录
///
/// 业务逻辑：
/// 1. 尝试读取 HOME 环境变量
/// 2. 失败时使用 /tmp/ghostcode 作为后备
fn dirs_or_home() -> PathBuf {
    // 尝试 HOME 环境变量
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".ghostcode");
    }

    // 后备路径
    PathBuf::from("/tmp/ghostcode")
}

/// 程序主入口
///
/// 业务逻辑：
/// 1. 解析命令行参数（clap）
/// 2. 构建 StartupConfig
/// 3. 初始化 tracing 日志
/// 4. 启动 tokio 运行时并执行 run_daemon
#[tokio::main]
async fn main() {
    // 初始化结构化日志（支持 RUST_LOG 环境变量控制日志级别）
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("ghostcoded=info".parse().unwrap()),
        )
        .init();

    // 解析命令行参数
    let args = Args::parse();

    // 构建 groups_dir（优先使用显式参数，否则基于 base_dir 推导）
    let groups_dir = args
        .groups_dir
        .unwrap_or_else(|| args.base_dir.join("groups"));

    let config = StartupConfig {
        base_dir: args.base_dir,
        groups_dir,
    };

    // 运行 Daemon 完整启动链路
    if let Err(e) = run_daemon(config).await {
        tracing::error!("Daemon 启动失败: {}", e);
        std::process::exit(1);
    }
}
