//! Daemon 启动链路
//!
//! 封装完整的 Daemon 启动流程：
//! 锁获取 -> 路径初始化 -> 清理残留 -> AppState -> bind socket -> 写 addr.json/PID -> 信号处理 -> serve_forever -> 退出清理
//!
//! 关键顺序约束（竞态修复）：
//! 必须先 bind socket，再写 addr.json。
//! 客户端读到 addr.json 时，socket 必然已经就绪可接受连接，
//! 消除 "addr.json 已存在但 socket 尚未 bind" 的竞态窗口。
//!
//! 参考: cccc/src/cccc/daemon_main.py - 单写者进程 + Unix Socket IPC
//!
//! @author Atlas.oi
//! @date 2026-03-04

use std::path::PathBuf;
use std::sync::Arc;

use ghostcode_types::addr::AddrDescriptor;
use tokio::net::UnixListener;

use crate::lock::try_acquire_singleton_lock;
use crate::paths::DaemonPaths;
use crate::process::{cleanup_stale_files, write_addr_descriptor, write_pid_file};
use crate::server::{AppState, DaemonConfig, serve_forever};

/// 加载配置并设置日志级别
///
/// 加载四层 TOML 配置（default > global > project > runtime），
/// 使用配置中的 observability.log_level 初始化 tracing subscriber。
/// 加载失败时不阻断启动，使用默认 info 级别。
///
/// @param base_dir - 基础目录（如 ~/.ghostcode/）
fn init_config_and_logging(base_dir: &std::path::Path) {
    // 尝试加载四层配置
    match ghostcode_config::load_effective_config(base_dir, None, None) {
        Ok(config) => {
            // 使用配置中的日志级别初始化 tracing
            let level = match config.observability.log_level.as_str() {
                "trace" => tracing::Level::TRACE,
                "debug" => tracing::Level::DEBUG,
                "warn" => tracing::Level::WARN,
                "error" => tracing::Level::ERROR,
                _ => tracing::Level::INFO,
            };
            // 使用 try_init 避免重复初始化（测试环境中多个 test 并行运行时可能多次调用）
            let _ = tracing_subscriber::fmt()
                .with_max_level(level)
                .with_target(false)
                .try_init();
            tracing::info!(
                "配置加载成功，日志级别: {}，最大 Actor 数: {}",
                config.observability.log_level,
                config.runtime.max_actors
            );
        }
        Err(e) => {
            // 配置加载失败不阻断启动，使用默认日志级别
            let _ = tracing_subscriber::fmt()
                .with_max_level(tracing::Level::INFO)
                .with_target(false)
                .try_init();
            tracing::warn!("配置加载失败（使用默认值）: {}", e);
        }
    }
}

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
/// 业务逻辑（严格顺序，消除竞态窗口）：
/// 1. 获取单实例锁（确保只有一个 Daemon 运行）
/// 2. 初始化路径（基于 base_dir 派生所有文件路径）
/// 3. 清理上次异常退出的残留文件
/// 4. 创建 AppState（共享状态容器）
/// 5. **先** bind Unix Socket（确认地址可用，进入 LISTEN 状态）
/// 6. **然后** 写入 addr.json（此时 socket 已就绪，客户端可安全连接）
/// 7. 写入 PID 文件（供运维工具查询进程状态）
/// 8. 设置信号处理（SIGTERM/SIGINT -> 优雅关闭）
/// 9. 运行 serve_forever（传入已绑定的 listener，阻塞直到收到关闭信号）
/// 10. 退出清理（addr.json 和 PID 文件在 serve_forever 清理 socket 后删除）
///
/// 关键约束：步骤 5 必须在步骤 6 之前完成，
/// 保证客户端读到 addr.json 时 socket 必然可连接。
///
/// @param config - 启动配置
/// @return 正常退出或错误
pub async fn run_daemon(config: StartupConfig) -> Result<(), StartupError> {
    // ============================================
    // 第零步：加载配置并初始化日志
    // 必须在所有其他操作之前，确保后续日志输出可被正确记录
    // ============================================
    init_config_and_logging(&config.base_dir);

    // ============================================
    // 第一步：派生所有路径
    // ============================================
    let paths = DaemonPaths::new(&config.base_dir);

    // 确保 daemon 目录存在
    std::fs::create_dir_all(&paths.daemon_dir)?;

    // ============================================
    // 第二步：获取单实例锁
    // 锁被持有期间，其他 Daemon 实例无法启动
    // 注意：必须在删除 addr.json 之前获取锁，
    // 否则会误删正在运行的 Daemon 的发现文件
    // ============================================
    let _lock_file = try_acquire_singleton_lock(&paths.lock)?;

    // ============================================
    // 第三步：清理残留文件（锁已持有，安全操作）
    // 处理上次异常退出遗留的 socket/addr/pid 文件
    // 包括删除旧的 addr.json，确保等待方必须等到新 addr.json 写入后才继续
    // ============================================
    let _ = std::fs::remove_file(&paths.addr);
    cleanup_stale_files(&paths.daemon_dir)?;

    // ============================================
    // 第四步：创建应用状态
    // ============================================
    std::fs::create_dir_all(&config.groups_dir)?;
    let state = Arc::new(AppState::new(config.groups_dir));

    // ============================================
    // 第五步：bind Unix Socket（先于写 addr.json）
    //
    // 竞态修复核心：在写 addr.json 之前完成 bind，
    // 确保客户端读到 addr.json 时 socket 已进入 LISTEN 状态。
    // bind 失败直接返回错误，不会写 addr.json（保持一致性）。
    // ============================================
    let listener = bind_socket(&paths.sock)?;

    // ============================================
    // 第六步：写入 addr.json
    // socket 已就绪，客户端现在可以安全连接
    //
    // 错误路径保护：如果写入 addr.json 或 PID 失败，
    // 必须清理已创建的 socket 文件，防止残留 socket 无对应发现文件
    // ============================================
    let pid = std::process::id();
    let sock_path_str = paths.sock.to_string_lossy().to_string();
    let descriptor = AddrDescriptor::new(&sock_path_str, pid, env!("CARGO_PKG_VERSION"));
    if let Err(e) = write_addr_descriptor(&paths.addr, &descriptor) {
        // addr.json 写入失败，清理 socket 文件防止孤儿残留
        let _ = std::fs::remove_file(&paths.sock);
        return Err(e.into());
    }

    // ============================================
    // 第七步：写入 PID 文件
    // 供运维工具（如 systemd）查询进程状态
    // ============================================
    if let Err(e) = write_pid_file(&paths.pid, pid) {
        // PID 写入失败，回滚已写入的 addr.json 和 socket 文件
        let _ = std::fs::remove_file(&paths.addr);
        let _ = std::fs::remove_file(&paths.sock);
        return Err(e.into());
    }

    // ============================================
    // 第八步：设置信号处理
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
    // 第九步：启动 serve_forever（阻塞）
    // 传入已绑定的 listener，监听 Unix Socket，处理 IPC 请求
    // serve_forever 在关闭信号触发后返回，并清理 socket 文件
    // ============================================
    let daemon_config = DaemonConfig {
        socket_path: paths.sock.clone(),
    };

    serve_forever(listener, daemon_config, state)
        .await
        .map_err(|e| StartupError::Server(e.to_string()))?;

    // ============================================
    // 第十步：退出清理
    // serve_forever 已清理 socket 文件
    // 清理 addr.json 和 PID 文件
    // ============================================
    let _ = std::fs::remove_file(&paths.addr);
    let _ = std::fs::remove_file(&paths.pid);

    Ok(())
}

/// 绑定 Unix Socket 并设置文件权限
///
/// 业务逻辑：
/// 1. 如有残留 socket 文件先删除（cleanup_stale_files 之后可能仍有残留）
/// 2. 调用 UnixListener::bind 进入 LISTEN 状态
/// 3. Unix 平台设置权限为 0o600（仅所有者可读写）
///
/// 此函数必须在 write_addr_descriptor 之前调用，
/// 确保 socket 就绪后才通知客户端。
///
/// @param sock_path - socket 文件路径
/// @return 已绑定的 UnixListener
fn bind_socket(sock_path: &std::path::Path) -> Result<UnixListener, StartupError> {
    // 确保不存在残留 socket 文件，bind 前必须清理
    // cleanup_stale_files 已处理大部分情况，这里作双重保障
    let _ = std::fs::remove_file(sock_path);

    let listener = UnixListener::bind(sock_path).map_err(|e| {
        StartupError::Io(std::io::Error::new(
            e.kind(),
            format!("bind Unix Socket 失败 {}: {}", sock_path.display(), e),
        ))
    })?;

    // 设置 socket 文件权限为 0o600（仅所有者可读写）
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(sock_path, perms)?;
    }

    Ok(listener)
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
