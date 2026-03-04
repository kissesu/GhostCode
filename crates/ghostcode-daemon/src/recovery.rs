//! @file recovery.rs
//! @description Daemon 僵尸进程清理与异常恢复逻辑
//!
//! 职责：
//! - reap_orphan_processes: Daemon 启动时扫描 base_dir，清理 stale pid/socket 文件
//! - handle_abnormal_exit: Actor 异常退出时根据退出码/信号决定恢复策略
//!
//! 业务规则：
//! - PID 对应进程不存在（kill -0 返回 ESRCH）→ stale，清理
//! - PID 对应进程存在 → 活跃，保留
//! - exit code = 1 → 临时错误 → Restart
//! - exit code > 1 → 致命错误（配置/代码问题）→ MarkFailed
//! - 信号终止（signal != None）→ 被外部强制终止 → Restart
//!
//! @author Atlas.oi
//! @date 2026-03-04

use std::path::Path;

/// 恢复动作枚举
///
/// Actor 异常退出后，由 handle_abnormal_exit 返回，
/// 调用方根据此枚举决定后续操作
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryAction {
    /// 重启 Actor（临时错误或信号终止）
    Restart,
    /// 标记为失败，不再重试（致命错误）
    MarkFailed {
        /// 失败原因描述
        reason: String,
    },
    /// 清理完成，无需进一步操作
    Cleaned,
}

/// 恢复错误类型
#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("PID 解析失败: {0}")]
    PidParse(String),
}

/// 回收孤儿进程记录
///
/// 在 Daemon 启动时调用，扫描 base_dir 下的 ghostcoded.pid 和 ghostcoded.sock 文件，
/// 清理不对应任何运行中进程的 stale 记录。
///
/// 业务逻辑：
/// 1. 检查 ghostcoded.pid 文件是否存在，读取 PID
/// 2. 检查 PID 对应的进程是否存在（kill(pid, 0) / /proc/pid 方式）
/// 3. 进程不存在：删除 pid 文件和 ghostcoded.sock 文件，记录清理日志
/// 4. 进程存在：保留文件，不做任何操作
/// 5. 返回已清理的记录描述列表
///
/// @param base_dir - Daemon 工作目录（如 ~/.ghostcode/daemon/）
/// @return 已清理的记录描述列表，失败时返回 RecoveryError
pub fn reap_orphan_processes(base_dir: &Path) -> Result<Vec<String>, RecoveryError> {
    let pid_file = base_dir.join("ghostcoded.pid");
    let sock_file = base_dir.join("ghostcoded.sock");

    // pid 文件不存在则无需清理
    if !pid_file.exists() {
        return Ok(Vec::new());
    }

    // ============================================
    // 第一步：读取 PID 文件，解析进程 ID
    // ============================================
    let pid_content = std::fs::read_to_string(&pid_file)?;
    let pid: u32 = pid_content
        .trim()
        .parse()
        .map_err(|e| RecoveryError::PidParse(format!("PID 文件内容 '{}' 解析失败: {}", pid_content.trim(), e)))?;

    // ============================================
    // 第二步：检查进程是否存活
    // 使用 kill(pid, 0) 探测，ESRCH 表示进程不存在
    // ============================================
    let process_alive = is_process_alive(pid);

    if process_alive {
        // 进程仍在运行，保留文件
        return Ok(Vec::new());
    }

    // ============================================
    // 第三步：进程已死亡，清理 stale 文件
    // 先清理 socket（客户端依赖），再清理 pid
    // ============================================
    let mut cleaned = Vec::new();

    // 清理 socket 文件（若存在）
    if sock_file.exists() {
        std::fs::remove_file(&sock_file)?;
        cleaned.push(format!("清理 stale socket: {}", sock_file.display()));
    }

    // 清理 pid 文件
    std::fs::remove_file(&pid_file)?;
    cleaned.push(format!("清理 stale pid (PID={}): {}", pid, pid_file.display()));

    Ok(cleaned)
}

/// 检查指定 PID 的进程是否存活
///
/// 业务逻辑：
/// - Unix 平台：使用 kill(pid, 0)，ESRCH 表示不存在，成功返回表示存在
/// - 非 Unix 平台：通过检查 /proc/{pid} 目录是否存在（回退方案）
/// - PID 为 0 时直接返回 false（无效 PID）
///
/// @param pid - 要检查的进程 ID
/// @return 进程是否存活
fn is_process_alive(pid: u32) -> bool {
    // PID 0 无效
    if pid == 0 {
        return false;
    }

    #[cfg(unix)]
    {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;

        // kill(pid, None) 等价于 kill(pid, 0)，仅探测进程是否存在，不发送实际信号
        // Ok(()) 表示进程存在；Err(ESRCH) 表示进程不存在；Err(EPERM) 表示无权限但进程存在
        match kill(Pid::from_raw(pid as i32), None) {
            Ok(()) => true,
            Err(nix::errno::Errno::EPERM) => true,  // 进程存在，只是无权限发信号
            Err(_) => false,                         // ESRCH 或其他错误，进程不存在
        }
    }

    #[cfg(not(unix))]
    {
        // 非 Unix 平台：通过 /proc/{pid} 检查（Linux 兼容路径）
        std::path::Path::new(&format!("/proc/{}", pid)).exists()
    }
}

/// 处理 Actor 异常退出
///
/// 根据退出状态（exit code / 信号）决定恢复策略：
///
/// 决策逻辑：
/// - 信号终止（signal != None）→ Restart（被外部强制终止，如 OOM Kill，应重启）
/// - exit code = 1 → Restart（临时错误，可能是资源竞争等瞬时问题）
/// - exit code > 1 → MarkFailed（配置错误/代码 Bug，重启无意义）
/// - exit code = None 且 signal = None → MarkFailed（未知原因，保守处理）
///
/// @param actor_id - Actor 标识符（仅用于日志）
/// @param exit_code - 退出码（进程正常退出时有值）
/// @param signal - 终止信号编号（被信号终止时有值）
/// @return 恢复动作
pub fn handle_abnormal_exit(actor_id: &str, exit_code: Option<i32>, signal: Option<i32>) -> RecoveryAction {
    // ============================================
    // 优先级 1：信号终止 → 重启
    // 被 SIGKILL/SIGTERM 等信号终止，通常是外部原因（OOM、systemd 等），应重启
    // ============================================
    if signal.is_some() {
        // 信号终止（SIGKILL/SIGTERM 等），被外部强制终止，应重启
        return RecoveryAction::Restart;
    }

    // ============================================
    // 优先级 2：根据 exit code 决定策略
    // ============================================
    match exit_code {
        // exit code = 1：临时错误（资源不足、网络超时等），重启
        Some(1) => RecoveryAction::Restart,

        // exit code > 1：配置错误/代码 Bug，标记失败（重启无意义）
        Some(code) if code > 1 => RecoveryAction::MarkFailed {
            reason: format!("Actor '{}' 退出码 {}，可能是配置错误或代码 Bug", actor_id, code),
        },

        // exit code <= 0（含 0，不应在异常路径出现，保守处理）
        Some(code) => RecoveryAction::MarkFailed {
            reason: format!("Actor '{}' 异常退出码 {}，无法确定原因", actor_id, code),
        },

        // 既无 exit code 也无 signal → 未知原因，保守标记失败
        None => RecoveryAction::MarkFailed {
            reason: format!("Actor '{}' 异常退出，原因未知（无 exit code 和 signal）", actor_id),
        },
    }
}

/// Actor 退出时的统一处理入口
///
/// 供 lifecycle.rs 在 Actor 退出时调用，返回受控的恢复动作
///
/// @param actor_id - Actor 标识符
/// @param exit_code - 退出码（可选）
/// @param signal - 终止信号（可选）
/// @return 恢复动作
pub fn on_actor_exit(actor_id: &str, exit_code: Option<i32>, signal: Option<i32>) -> RecoveryAction {
    handle_abnormal_exit(actor_id, exit_code, signal)
}
