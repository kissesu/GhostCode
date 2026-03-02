//! Daemon 启动链路
//!
//! 封装完整的 Daemon 启动流程：
//! 锁获取 -> 路径初始化 -> 清理残留 -> AppState -> addr.json/PID -> 信号处理 -> serve_forever -> 退出清理
//!
//! 参考: cccc/src/cccc/daemon_main.py - 单写者进程 + Unix Socket IPC
//!
//! @author Atlas.oi
//! @date 2026-03-02

use std::path::PathBuf;
use std::sync::Arc;

use ghostcode_types::addr::AddrDescriptor;

use crate::lock::try_acquire_singleton_lock;
use crate::paths::DaemonPaths;
use crate::process::{cleanup_stale_files, write_addr_descriptor, write_pid_file};
use crate::server::{AppState, DaemonConfig, serve_forever};

/// Daemon 启动配置
///
/// 包含启动 Daemon 所需的全部路径信息
#[derive(Debug, Clone)]
pub struct StartupConfig {
    /// 基础目录（如 ~/.ghostcode/）
    /// DaemonPaths 从此目录派生所有子路径
    pub base_dir: PathBuf,

    /// groups 根目录路径，用于加载 group.yaml
    pub groups_dir: PathBuf,
}

/// Daemon 启动错误类型
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    #[error("获取单实例锁失败: {0}")]
    Lock(#[from] crate::lock::LockError),

    #[error("进程管理操作失败: {0}")]
    Process(#[from] crate::process::ProcessError),

    #[error("服务器启动失败: {0}")]
    Server(String),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
}

/// 运行 Daemon 完整启动链路
///
/// 业务逻辑：
/// 1. 获取单实例锁（确保只有一个 Daemon 运行）
/// 2. 初始化路径（基于 base_dir 派生所有文件路径）
/// 3. 清理上次异常退出的残留文件
/// 4. 创建 AppState（共享状态容器）
/// 5. 写入 addr.json（供客户端查找 socket 路径）
/// 6. 写入 PID 文件（供运维工具查询进程状态）
/// 7. 设置信号处理（SIGTERM/SIGINT -> 优雅关闭）
/// 8. 运行 serve_forever（阻塞直到收到关闭信号）
/// 9. 退出清理（addr.json 和 PID 文件由 serve_forever 清理 socket 后删除）
///
/// @param config - 启动配置
/// @return 正常退出或错误
pub async fn run_daemon(config: StartupConfig) -> Result<(), StartupError> {
    // ============================================
    // 第一步：派生所有路径
    // ============================================
    let paths = DaemonPaths::new(&config.base_dir);

    // 确保 daemon 目录存在
    std::fs::create_dir_all(&paths.daemon_dir)?;

    // ============================================
    // 预清理：在获取锁之前立即删除 addr.json
    //
    // 原因：addr.json 存在时，客户端/测试辅助函数会认为 Daemon 已就绪。
    // 如果 run_daemon 在 tokio::spawn 中异步执行，cleanup_stale_files
    // 可能晚于调用方的等待检查才运行，导致竞态条件（测试误判就绪）。
    // 提前删除 addr.json 确保等待方必须等到新 addr.json 写入后才继续。
    // ============================================
    let _ = std::fs::remove_file(&paths.addr);

    // ============================================
    // 第二步：获取单实例锁
    // 锁被持有期间，其他 Daemon 实例无法启动
    // ============================================
    let _lock_file = try_acquire_singleton_lock(&paths.lock)?;

    // ============================================
    // 第三步：清理残留文件
    // 处理上次异常退出遗留的 socket/addr/pid 文件
    // (addr.json 已在预清理步骤中删除)
    // ============================================
    cleanup_stale_files(&paths.daemon_dir)?;

    // ============================================
    // 第四步：创建应用状态
    // ============================================
    std::fs::create_dir_all(&config.groups_dir)?;
    let state = Arc::new(AppState::new(config.groups_dir));

    // ============================================
    // 第五步：写入 addr.json
    // 客户端通过读取此文件获取 socket 路径
    // ============================================
    let pid = std::process::id();
    let sock_path_str = paths.sock.to_string_lossy().to_string();
    let descriptor = AddrDescriptor::new(&sock_path_str, pid, env!("CARGO_PKG_VERSION"));
    write_addr_descriptor(&paths.addr, &descriptor)?;

    // ============================================
    // 第六步：写入 PID 文件
    // 供运维工具（如 systemd）查询进程状态
    // ============================================
    write_pid_file(&paths.pid, pid)?;

    // ============================================
    // 第七步：设置信号处理
    // SIGTERM/SIGINT 触发优雅关闭
    // ============================================
    {
        let state_for_signal = Arc::clone(&state);
        tokio::spawn(async move {
            wait_for_shutdown_signal().await;
            state_for_signal.trigger_shutdown();
        });
    }

    // ============================================
    // 第八步：启动 serve_forever（阻塞）
    // 监听 Unix Socket，处理 IPC 请求
    // serve_forever 在关闭信号触发后返回，并清理 socket 文件
    // ============================================
    let daemon_config = DaemonConfig {
        socket_path: paths.sock.clone(),
    };

    serve_forever(daemon_config, state)
        .await
        .map_err(|e| StartupError::Server(e.to_string()))?;

    // ============================================
    // 第九步：退出清理
    // serve_forever 已清理 socket 文件
    // 清理 addr.json 和 PID 文件
    // ============================================
    let _ = std::fs::remove_file(&paths.addr);
    let _ = std::fs::remove_file(&paths.pid);

    Ok(())
}

/// 等待系统关闭信号（SIGTERM 或 SIGINT / Ctrl+C）
///
/// 跨平台实现：
/// - Unix: 同时监听 SIGTERM 和 SIGINT
/// - 其他平台: 仅监听 Ctrl+C
///
/// 信号注册失败时记录错误并回退到 ctrl_c()，避免 panic
async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        // 尝试注册 SIGTERM 和 SIGINT，失败时回退到 ctrl_c
        let sigterm_result = signal(SignalKind::terminate());
        let sigint_result = signal(SignalKind::interrupt());

        match (sigterm_result, sigint_result) {
            (Ok(mut sigterm), Ok(mut sigint)) => {
                tokio::select! {
                    _ = sigterm.recv() => {}
                    _ = sigint.recv() => {}
                }
            }
            _ => {
                // 信号注册失败，回退到 ctrl_c
                tracing::warn!("Unix 信号注册失败，回退到 ctrl_c 监听");
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
