//! ghostcode-web 服务器入口点
//!
//! 负责 CLI 参数解析、Daemon 冷启动自举、tracing 日志初始化、
//! CORS 中间件配置、HTTP 服务器启动和优雅关停（SIGTERM / Ctrl-C）。
//!
//! 冷启动自举流程（与 ghostcode-mcp 保持一致）：
//! 1. 尝试读取 {base_dir}/daemon/ghostcoded.addr.json 获取 Daemon socket 路径
//! 2. 若 addr.json 不存在，自动启动 Daemon（后台 spawn ghostcoded）
//! 3. 轮询等待 addr.json 出现（最多 5 秒，200ms 间隔）
//! 4. 使用解析出的 socket 路径构建 WebState
//!
//! @author Atlas.oi
//! @date 2026-03-05

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use ghostcode_web::server::{build_cors_layer, create_router};
use ghostcode_web::state::WebState;

/// Daemon 自举等待超时时间（毫秒）
const DAEMON_BOOT_TIMEOUT_MS: u64 = 5000;

/// Daemon 自举轮询间隔（毫秒）
const DAEMON_POLL_INTERVAL_MS: u64 = 200;

// ============================================
// CLI 参数结构体定义
// 与 tests/main_cli_test.rs 中的契约保持一致。
// 当 main.rs 参数发生变化时，必须同步更新测试中的 Args 定义。
// ============================================

#[derive(Parser, Debug, Clone)]
#[command(
    name = "ghostcode-web",
    about = "GhostCode Web Dashboard HTTP 服务器",
    version
)]
pub struct Args {
    /// HTTP 服务器绑定地址
    #[arg(long, default_value = "127.0.0.1:7070")]
    pub bind: SocketAddr,

    /// 数据根目录（默认 ~/.ghostcode）
    #[arg(long, value_name = "DIR")]
    pub base_dir: Option<PathBuf>,

    /// Daemon Unix Socket 路径（默认从 addr.json 自动解析，或 {base_dir}/daemon/ghostcoded.sock）
    #[arg(long, value_name = "SOCKET")]
    pub daemon_socket: Option<PathBuf>,

    /// CORS 允许的源（可多次指定）
    #[arg(long, default_value = "http://localhost:5173")]
    pub cors_origin: Vec<String>,

    /// 关停超时秒数（超时后强制终止所有连接包括 SSE）
    #[arg(long, default_value = "3")]
    pub shutdown_grace_secs: u64,
}

// ============================================
// 辅助函数：获取默认数据根目录
// 优先使用用户 Home 目录下的 .ghostcode，
// 若无法获取 Home 目录则退回到当前目录。
// ============================================

fn default_base_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ghostcode")
}

// ============================================
// 初始化 tracing 日志
//
// 使用 try_init 避免多次初始化导致 panic（测试场景中多次初始化是正常的）。
// 日志级别从环境变量 RUST_LOG 读取，默认为 "info"。
// 输出格式为人类可读的文本格式（非 JSON），适合开发和终端使用。
// ============================================

fn init_tracing() {
    // 使用 try_init 而非 init，允许多次调用（测试场景下不会 panic）
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}

// ============================================
// 等待系统关停信号
//
// 在 Unix 系统上同时监听 SIGTERM 和 Ctrl-C（SIGINT）：
//   - SIGTERM：由系统、容器编排（如 k8s）或 init 系统发送
//   - Ctrl-C（SIGINT）：用户在终端手动终止
//
// 在非 Unix 系统（Windows）上仅监听 Ctrl-C。
// ============================================

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        // Unix 系统：同时监听 SIGTERM 和 Ctrl-C
        let mut sigterm = tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::terminate(),
        )
        .expect("注册 SIGTERM 信号处理器失败");

        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                result.expect("注册 Ctrl-C 信号处理器失败");
                tracing::info!("[GhostCode Web] 收到 Ctrl-C 信号，开始关停...");
            }
            _ = sigterm.recv() => {
                tracing::info!("[GhostCode Web] 收到 SIGTERM 信号，开始关停...");
            }
        }
    }

    #[cfg(not(unix))]
    {
        // 非 Unix 系统（Windows）：仅监听 Ctrl-C
        tokio::signal::ctrl_c()
            .await
            .expect("注册 Ctrl-C 信号处理器失败");
        tracing::info!("[GhostCode Web] 收到 Ctrl-C 信号，开始关停...");
    }
}

// ============================================
// Daemon 冷启动自举
//
// 与 ghostcode-mcp 保持一致的自举流程：
// 1. 尝试读取 addr.json 获取 Daemon socket 路径
// 2. addr.json 不存在时自动启动 Daemon
// 3. 轮询等待 Daemon 就绪
//
// addr.json 路径: {base_dir}/daemon/ghostcoded.addr.json
// 默认 socket 路径: {base_dir}/daemon/ghostcoded.sock
// ============================================

/// 从 addr.json 解析 Daemon socket 路径
///
/// 读取 {base_dir}/daemon/ghostcoded.addr.json 并提取 "path" 字段
///
/// @param base_dir - GhostCode 基准目录（如 ~/.ghostcode/）
/// @return Socket 文件的 PathBuf
fn resolve_daemon_addr(base_dir: &Path) -> Result<PathBuf> {
    let addr_path = base_dir.join("daemon").join("ghostcoded.addr.json");

    let content = std::fs::read_to_string(&addr_path)
        .with_context(|| format!("无法读取 addr.json: {}", addr_path.display()))?;

    let parsed: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| "addr.json 格式无效")?;

    let sock_path = parsed
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("addr.json 缺少 path 字段"))?;

    Ok(PathBuf::from(sock_path))
}

/// 尝试自动启动 Daemon 守护进程
///
/// 在 {base_dir}/bin/ 下查找 ghostcoded 二进制，
/// 以后台进程方式 spawn（detached，stdin/stdout/stderr 置空）
///
/// @param base_dir - GhostCode 基准目录
fn try_spawn_daemon(base_dir: &Path) -> Result<()> {
    let daemon_bin = base_dir.join("bin").join("ghostcoded");

    if !daemon_bin.exists() {
        anyhow::bail!(
            "Daemon 二进制不存在: {}，请运行 ghostcode init 安装",
            daemon_bin.display()
        );
    }

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
/// 以 200ms 间隔轮询 addr.json，超过 5 秒则超时报错
///
/// @param base_dir - GhostCode 基准目录
/// @return 解析出的 Daemon socket 路径
async fn wait_for_daemon_addr(base_dir: &Path) -> Result<PathBuf> {
    let poll_interval = Duration::from_millis(DAEMON_POLL_INTERVAL_MS);
    let timeout = Duration::from_millis(DAEMON_BOOT_TIMEOUT_MS);
    let start = std::time::Instant::now();

    loop {
        if let Ok(addr) = resolve_daemon_addr(base_dir) {
            return Ok(addr);
        }

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

/// 执行 Daemon 冷启动自举
///
/// 业务逻辑：
/// 1. 先尝试直接读取 addr.json（热路径，Daemon 已运行时零延迟）
/// 2. 若不存在，自动启动 Daemon 并等待 addr.json 出现
/// 3. 返回解析出的 socket 路径
///
/// @param base_dir - GhostCode 基准目录
/// @return Daemon socket 路径
async fn bootstrap_daemon(base_dir: &Path) -> Result<PathBuf> {
    // 热路径：Daemon 已运行，直接读取 addr.json
    if let Ok(addr) = resolve_daemon_addr(base_dir) {
        tracing::info!("[GhostCode Web] Daemon 已运行，socket: {}", addr.display());
        return Ok(addr);
    }

    // 冷路径：Daemon 未运行，尝试自动启动
    tracing::info!("[GhostCode Web] Daemon 未运行，尝试自动启动...");

    if let Err(e) = try_spawn_daemon(base_dir) {
        tracing::warn!("[GhostCode Web] 自动启动 Daemon 失败: {}", e);
        // 启动失败仍然等待，可能有其他进程正在启动 Daemon
    }

    // 轮询等待 Daemon 就绪
    wait_for_daemon_addr(base_dir)
        .await
        .context("Daemon 自举失败，请手动运行 ghostcoded 或 ghostcode init")
}

// ============================================
// 端口占用清理
//
// 启动前检测目标端口是否被旧的 ghostcode-web 进程占用。
// 如果是同名进程（ghostcode-web），发送 SIGTERM 并等待退出。
// 避免重启时手动清理端口的麻烦。
// ============================================

/// 清理占用目标端口的旧 ghostcode-web 进程
///
/// 业务逻辑：
/// 1. 尝试连接目标地址，连接成功说明端口被占用
/// 2. 通过 lsof 查找占用端口的进程 PID
/// 3. 只清理进程名为 ghostcode-web 的进程（避免误杀其他服务）
/// 4. 发送 SIGTERM 并等待最多 3 秒
///
/// @param addr - 目标绑定地址
async fn kill_stale_listener(addr: SocketAddr) {
    // 先尝试连接，如果连不上说明端口空闲，直接返回
    if tokio::net::TcpStream::connect(addr).await.is_err() {
        return;
    }

    tracing::info!(
        "[GhostCode Web] 端口 {} 被占用，尝试清理旧进程...",
        addr.port()
    );

    // 使用 lsof 查找占用端口的进程
    let output = match std::process::Command::new("lsof")
        .args(["-ti", &format!(":{}", addr.port())])
        .output()
    {
        Ok(o) => o,
        Err(_) => return, // lsof 不可用，跳过
    };

    let pids_str = String::from_utf8_lossy(&output.stdout);
    let my_pid = std::process::id();

    for pid_str in pids_str.trim().lines() {
        let pid: u32 = match pid_str.trim().parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        // 跳过自己
        if pid == my_pid {
            continue;
        }

        // 检查进程名是否为 ghostcode-web（避免误杀其他服务）
        let comm_output = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output();

        let is_ghostcode_web = match &comm_output {
            Ok(o) => {
                let comm = String::from_utf8_lossy(&o.stdout);
                comm.trim().ends_with("ghostcode-web")
            }
            Err(_) => false,
        };

        if !is_ghostcode_web {
            tracing::warn!(
                "[GhostCode Web] 端口 {} 被非 ghostcode-web 进程 (PID {}) 占用，跳过清理",
                addr.port(),
                pid
            );
            continue;
        }

        // 发送 SIGTERM（W1 修复：检查返回值并 log 错误）
        //
        // SAFETY 风险评估（C2-review）：
        // libc::kill 存在理论上的 PID 重用竞态：从 lsof 获取 PID 到此处发送
        // SIGTERM 的时间窗口内，目标进程可能退出且 PID 被 OS 回收分配给新进程。
        // 实际风险极低，原因：
        //   1. 上方 ps -o comm= 已验证进程名为 "ghostcode-web"
        //   2. PID 重用需要短时间内大量进程创建/销毁（开发环境概率极低）
        //   3. 即使误杀，也只是发送 SIGTERM（可被目标进程优雅处理）
        //   4. 此代码仅在 ghostcode-web 启动时执行一次，非热路径
        // 替代方案（pidfd/kqueue）对开发工具过度工程化，不采用。
        tracing::info!(
            "[GhostCode Web] 终止旧 ghostcode-web 进程 (PID {})",
            pid
        );
        let kill_result = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
        if kill_result != 0 {
            let errno = std::io::Error::last_os_error();
            tracing::warn!(
                "[GhostCode Web] 发送 SIGTERM 到 PID {} 失败: {}",
                pid,
                errno
            );
            continue;
        }

        // 等待进程退出（最多 3 秒）
        for _ in 0..30 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            // 检查进程是否已退出（kill 0 不发信号，只检查进程存在性）
            // SAFETY：signal=0 不发送任何信号，仅检查 PID 是否存在且有权限访问
            let alive = unsafe { libc::kill(pid as i32, 0) == 0 };
            if !alive {
                tracing::info!("[GhostCode Web] 旧进程 (PID {}) 已退出", pid);
                // 等待端口释放
                tokio::time::sleep(Duration::from_millis(200)).await;
                return;
            }
        }

        tracing::warn!(
            "[GhostCode Web] 旧进程 (PID {}) 未在 3 秒内退出",
            pid
        );
    }
}

// ============================================
// 程序入口
//
// 启动流程：
//   1. 初始化 tracing 日志
//   2. 解析 CLI 参数，派生运行时路径
//   3. Daemon 冷启动自举（解析或自动启动 Daemon 获取 socket 路径）
//   4. 构建 WebState + CORS 中间件 + axum Router
//   5. 清理旧进程 + 绑定 TCP 端口
//   6. 启动 HTTP 服务器，配置优雅关停
//   7. 等待关停信号（SIGTERM / Ctrl-C）
// ============================================

#[tokio::main]
async fn main() -> Result<()> {
    // ============================================
    // 第一步：初始化 tracing 日志
    // ============================================
    init_tracing();

    // ============================================
    // 第二步：解析 CLI 参数，派生运行时路径
    // ============================================
    let args = Args::parse();
    let base_dir = args.base_dir.unwrap_or_else(default_base_dir);

    // ============================================
    // 第三步：解析 Daemon socket 路径
    //
    // 优先级：
    //   1. --daemon-socket 显式指定 → 直接使用
    //   2. 从 addr.json 自动解析（含冷启动自举）
    // ============================================
    let daemon_socket = if let Some(explicit_socket) = args.daemon_socket {
        tracing::info!("[GhostCode Web] 使用显式 socket 路径: {}", explicit_socket.display());
        explicit_socket
    } else {
        bootstrap_daemon(&base_dir).await?
    };

    // ============================================
    // 第四步：构建 WebState + CORS 中间件 + axum Router
    // ============================================
    let state = WebState::with_socket(base_dir.clone(), daemon_socket.clone());
    let cors = build_cors_layer(&args.cors_origin);
    let app = create_router(state).layer(cors);

    // ============================================
    // 第五步：绑定 TCP 端口
    //
    // 先检测端口是否被占用，如果是旧的 ghostcode-web 进程则自动清理
    // 避免重启时因端口占用导致启动失败
    // ============================================
    kill_stale_listener(args.bind).await;

    let listener = tokio::net::TcpListener::bind(args.bind)
        .await
        .with_context(|| format!("绑定地址失败: {}", args.bind))?;

    // 输出启动信息
    let actual_addr = listener.local_addr().context("获取本地地址失败")?;
    tracing::info!("[GhostCode Web] 监听 http://{}", actual_addr);
    tracing::info!("[GhostCode Web] 数据目录: {}", base_dir.display());
    tracing::info!("[GhostCode Web] Daemon socket: {}", daemon_socket.display());
    tracing::info!("[GhostCode Web] CORS 源: {:?}", args.cors_origin);

    // ============================================
    // 第六步：启动 HTTP 服务器，配置关停机制
    //
    // 关停流程：
    // 1. 收到 Ctrl-C/SIGTERM → 停止接受新连接
    // 2. 等待 shutdown_grace_secs 秒让进行中的 REST 请求完成
    // 3. 超时后强制终止所有连接（包括 SSE 长连接）
    //
    // SSE 连接必须在关停时强制切断，否则端口无法释放，
    // 下次启动会端口冲突。这是服务生命周期的基本闭环。
    // ============================================
    let grace = Duration::from_secs(args.shutdown_grace_secs);
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let server = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            wait_for_shutdown_signal().await;
            let _ = shutdown_tx.send(());
        });

    tokio::select! {
        result = server => {
            match result {
                Ok(()) => tracing::info!("[GhostCode Web] 服务器已关停"),
                Err(e) => {
                    tracing::error!("[GhostCode Web] 服务器异常退出: {}", e);
                    return Err(e.into());
                }
            }
        }
        _ = async {
            let _ = shutdown_rx.await;
            tokio::time::sleep(grace).await;
        } => {
            tracing::info!(
                "[GhostCode Web] 关停超时（{}s），强制终止所有连接",
                args.shutdown_grace_secs
            );
        }
    }

    Ok(())
}
