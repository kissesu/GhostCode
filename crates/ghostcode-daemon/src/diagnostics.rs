//! @file diagnostics.rs
//! @description 生产可观测性和诊断收集模块
//!
//! 职责：
//! - collect_diagnostics: 收集五类诊断项（socket/pid/lock/config/recent_errors）
//! - determine_health_status: 根据诊断项判断整体健康状态（Ready/Degraded/Down）
//!
//! 健康状态判断规则：
//! - 全部 Ok → Ready
//! - 有 Warning 或非关键 Error（config/lock/recent_errors）→ Degraded
//! - 关键项 Error（socket 或 pid）→ Down
//!
//! @author Atlas.oi
//! @date 2026-03-04

use std::path::Path;

// ============================================
// 公共类型定义
// ============================================

/// 整体健康状态枚举
///
/// - Ready: 所有关键组件正常运行
/// - Degraded: 部分非关键组件异常，系统仍可工作
/// - Down: 关键组件（socket/pid）故障，系统不可用
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    /// 所有诊断项正常
    Ready,
    /// 部分非关键诊断项异常
    Degraded,
    /// 关键诊断项（socket/pid）异常
    Down,
}

/// 单项诊断状态
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemStatus {
    /// 正常
    Ok,
    /// 警告（非致命）
    Warning,
    /// 错误
    Error,
    /// 未知（通常因目录不存在或权限问题）
    Unknown,
}

/// 单项诊断结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiagnosticItem {
    /// 诊断类别：socket / pid / lock / config / recent_errors
    pub category: String,
    /// 诊断状态
    pub status: ItemStatus,
    /// 状态摘要说明
    pub message: String,
    /// 详细信息（可选）
    pub details: Option<String>,
}

/// 完整诊断报告
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiagnosticsReport {
    /// 五类诊断项列表
    pub items: Vec<DiagnosticItem>,
    /// 整体健康状态
    pub health: HealthStatus,
    /// 报告生成时间戳（ISO 8601 格式）
    pub timestamp: String,
}

// ============================================
// 公共函数
// ============================================

/// 收集完整诊断信息
///
/// 检查五个维度：
/// 1. socket — Unix socket 文件是否存在
/// 2. pid   — PID 文件是否存在且对应进程存活
/// 3. lock  — 锁文件状态
/// 4. config — 配置文件是否可解析
/// 5. recent_errors — 最近恢复/错误事件记录
///
/// 即使 base_dir 不存在，也不会 panic，各项返回 Unknown 或 Error 状态。
///
/// @param base_dir - Daemon 工作目录（如 ~/.ghostcode/daemon/）
/// @return 完整诊断报告（含各项状态和整体健康状态）
pub fn collect_diagnostics(base_dir: &Path) -> DiagnosticsReport {
    // ============================================
    // 收集五类诊断项
    // ============================================
    let items = vec![
        check_socket(base_dir),
        check_pid(base_dir),
        check_lock(base_dir),
        check_config(base_dir),
        check_recent_errors(base_dir),
    ];

    // ============================================
    // 计算整体健康状态
    // ============================================
    let health = determine_health_status(&items);

    // ============================================
    // 生成 ISO 8601 格式时间戳
    // 格式：YYYY-MM-DDTHH:MM:SSZ（UTC）
    // ============================================
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    DiagnosticsReport {
        items,
        health,
        timestamp,
    }
}

/// 根据诊断项列表判断整体健康状态
///
/// 判断规则（优先级从高到低）：
/// 1. socket 或 pid 任一为 Error → Down（关键组件故障，系统不可用）
/// 2. 任意项为 Warning 或 Error → Degraded（部分功能受损）
/// 3. 全部为 Ok → Ready
///
/// @param items - 诊断项列表
/// @return 整体健康状态
pub fn determine_health_status(items: &[DiagnosticItem]) -> HealthStatus {
    // 关键诊断项：socket 或 pid 故障直接返回 Down
    for item in items {
        if (item.category == "socket" || item.category == "pid")
            && item.status == ItemStatus::Error
        {
            return HealthStatus::Down;
        }
    }

    // 存在任意 Warning、Error 或 Unknown → Degraded
    // Unknown 表示无法判定状态（目录不存在、权限不足等），不应视为健康
    for item in items {
        if item.status == ItemStatus::Warning
            || item.status == ItemStatus::Error
            || item.status == ItemStatus::Unknown
        {
            return HealthStatus::Degraded;
        }
    }

    // 全部 Ok
    HealthStatus::Ready
}

// ============================================
// 私有诊断检查函数
// ============================================

/// 检查 Unix socket 文件状态
///
/// 仅检查文件是否存在（实际可连接性测试需要 tokio，此处保持同步）
/// - 文件存在 → Ok
/// - 文件不存在但目录存在 → Error（daemon 可能未运行）
/// - 目录不存在 → Unknown
///
/// @param base_dir - Daemon 工作目录
fn check_socket(base_dir: &Path) -> DiagnosticItem {
    if !base_dir.exists() {
        return DiagnosticItem {
            category: "socket".to_string(),
            status: ItemStatus::Unknown,
            message: "工作目录不存在，无法检查 socket 文件".to_string(),
            details: Some(format!("路径: {}", base_dir.display())),
        };
    }

    let sock_file = base_dir.join("ghostcoded.sock");
    if sock_file.exists() {
        DiagnosticItem {
            category: "socket".to_string(),
            status: ItemStatus::Ok,
            message: "socket 文件存在".to_string(),
            details: Some(format!("路径: {}", sock_file.display())),
        }
    } else {
        DiagnosticItem {
            category: "socket".to_string(),
            status: ItemStatus::Error,
            message: "socket 文件不存在，daemon 可能未运行".to_string(),
            details: Some(format!("期望路径: {}", sock_file.display())),
        }
    }
}

/// 检查 PID 文件状态及进程存活性
///
/// 业务逻辑：
/// 1. PID 文件不存在 → Error
/// 2. PID 文件存在但内容无法解析 → Error
/// 3. PID 文件存在且进程存活 → Ok
/// 4. PID 文件存在但进程已死亡 → Warning（stale pid 文件）
///
/// @param base_dir - Daemon 工作目录
fn check_pid(base_dir: &Path) -> DiagnosticItem {
    if !base_dir.exists() {
        return DiagnosticItem {
            category: "pid".to_string(),
            status: ItemStatus::Unknown,
            message: "工作目录不存在，无法检查 PID 文件".to_string(),
            details: None,
        };
    }

    let pid_file = base_dir.join("ghostcoded.pid");

    if !pid_file.exists() {
        return DiagnosticItem {
            category: "pid".to_string(),
            status: ItemStatus::Error,
            message: "PID 文件不存在，daemon 可能未运行".to_string(),
            details: Some(format!("期望路径: {}", pid_file.display())),
        };
    }

    // 读取并解析 PID
    let content = match std::fs::read_to_string(&pid_file) {
        Ok(c) => c,
        Err(e) => {
            return DiagnosticItem {
                category: "pid".to_string(),
                status: ItemStatus::Error,
                message: "PID 文件读取失败".to_string(),
                details: Some(format!("错误: {}", e)),
            };
        }
    };

    let pid: u32 = match content.trim().parse() {
        Ok(p) => p,
        Err(e) => {
            return DiagnosticItem {
                category: "pid".to_string(),
                status: ItemStatus::Error,
                message: "PID 文件内容无法解析".to_string(),
                details: Some(format!("内容: '{}', 错误: {}", content.trim(), e)),
            };
        }
    };

    // 检查进程是否存活
    if is_process_alive(pid) {
        DiagnosticItem {
            category: "pid".to_string(),
            status: ItemStatus::Ok,
            message: format!("daemon 进程存活 (PID={})", pid),
            details: Some(format!("路径: {}", pid_file.display())),
        }
    } else {
        DiagnosticItem {
            category: "pid".to_string(),
            status: ItemStatus::Warning,
            message: format!("PID 文件存在但进程已死亡 (PID={})，可能是 stale 文件", pid),
            details: Some(format!("路径: {}", pid_file.display())),
        }
    }
}

/// 检查锁文件状态
///
/// - 锁文件存在 → Ok（正常锁定）
/// - 锁文件不存在 → Warning（daemon 未持有锁，可能未运行）
/// - 目录不存在 → Unknown
///
/// @param base_dir - Daemon 工作目录
fn check_lock(base_dir: &Path) -> DiagnosticItem {
    if !base_dir.exists() {
        return DiagnosticItem {
            category: "lock".to_string(),
            status: ItemStatus::Unknown,
            message: "工作目录不存在，无法检查锁文件".to_string(),
            details: None,
        };
    }

    let lock_file = base_dir.join("ghostcoded.lock");

    if lock_file.exists() {
        DiagnosticItem {
            category: "lock".to_string(),
            status: ItemStatus::Ok,
            message: "锁文件存在，daemon 正常持有锁".to_string(),
            details: Some(format!("路径: {}", lock_file.display())),
        }
    } else {
        DiagnosticItem {
            category: "lock".to_string(),
            status: ItemStatus::Warning,
            message: "锁文件不存在，daemon 可能未运行或异常退出".to_string(),
            details: Some(format!("期望路径: {}", lock_file.display())),
        }
    }
}

/// 检查配置文件可解析性
///
/// 业务逻辑：
/// 1. 配置文件不存在 → Warning（使用默认配置）
/// 2. 配置文件存在且可读取 → Ok
/// 3. 配置文件存在但读取失败 → Error
///
/// 注意：仅检查文件可读性，不深度解析 TOML 结构，避免依赖 ghostcode-config
///
/// @param base_dir - Daemon 工作目录
fn check_config(base_dir: &Path) -> DiagnosticItem {
    if !base_dir.exists() {
        return DiagnosticItem {
            category: "config".to_string(),
            status: ItemStatus::Unknown,
            message: "工作目录不存在，无法检查配置文件".to_string(),
            details: None,
        };
    }

    let config_file = base_dir.join("config.toml");

    if !config_file.exists() {
        return DiagnosticItem {
            category: "config".to_string(),
            status: ItemStatus::Warning,
            message: "配置文件不存在，将使用默认配置".to_string(),
            details: Some(format!("期望路径: {}", config_file.display())),
        };
    }

    // 尝试读取配置文件内容（验证文件可访问）
    match std::fs::read_to_string(&config_file) {
        Ok(_) => DiagnosticItem {
            category: "config".to_string(),
            status: ItemStatus::Ok,
            message: "配置文件存在且可读".to_string(),
            details: Some(format!("路径: {}", config_file.display())),
        },
        Err(e) => DiagnosticItem {
            category: "config".to_string(),
            status: ItemStatus::Error,
            message: "配置文件读取失败".to_string(),
            details: Some(format!("路径: {}, 错误: {}", config_file.display(), e)),
        },
    }
}

/// 检查最近错误/恢复事件记录
///
/// 扫描 base_dir 下的错误日志或恢复记录文件：
/// - recovery.log 或 errors.log 文件不存在 → Ok（无错误记录）
/// - 文件存在但为空 → Ok（无错误）
/// - 文件存在且非空 → Warning（存在历史错误记录）
/// - 目录不存在 → Unknown
///
/// @param base_dir - Daemon 工作目录
fn check_recent_errors(base_dir: &Path) -> DiagnosticItem {
    if !base_dir.exists() {
        return DiagnosticItem {
            category: "recent_errors".to_string(),
            status: ItemStatus::Unknown,
            message: "工作目录不存在，无法检查错误记录".to_string(),
            details: None,
        };
    }

    // 检查常见的错误/恢复日志文件
    let candidates = ["recovery.log", "errors.log", "daemon.err"];
    let mut found_errors = Vec::new();

    for filename in &candidates {
        let log_file = base_dir.join(filename);
        if log_file.exists() {
            if let Ok(content) = std::fs::read_to_string(&log_file) {
                let line_count = content.lines().count();
                if line_count > 0 {
                    found_errors.push(format!("{} ({} 条记录)", filename, line_count));
                }
            }
        }
    }

    if found_errors.is_empty() {
        DiagnosticItem {
            category: "recent_errors".to_string(),
            status: ItemStatus::Ok,
            message: "无错误/恢复事件记录".to_string(),
            details: None,
        }
    } else {
        DiagnosticItem {
            category: "recent_errors".to_string(),
            status: ItemStatus::Warning,
            message: format!("发现 {} 个错误日志文件", found_errors.len()),
            details: Some(found_errors.join(", ")),
        }
    }
}

/// 检查指定 PID 的进程是否存活
///
/// - Unix 平台：使用 kill(pid, 0) 探测
/// - PID 为 0 → 直接返回 false（无效 PID）
///
/// @param pid - 要检查的进程 ID
fn is_process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }

    #[cfg(unix)]
    {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;

        // kill(pid, None) 仅探测进程存在性，不发送实际信号
        // Ok(()) 或 Err(EPERM) → 进程存在；Err(ESRCH) → 进程不存在
        match kill(Pid::from_raw(pid as i32), None) {
            Ok(()) => true,
            Err(nix::errno::Errno::EPERM) => true,
            Err(_) => false,
        }
    }

    #[cfg(not(unix))]
    {
        std::path::Path::new(&format!("/proc/{}", pid)).exists()
    }
}
