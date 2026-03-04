//! @file doctor_test.rs
//! @description P8-T4 诊断系统测试 — TDD Red 阶段
//!
//! 测试覆盖范围：
//! 1. 五类诊断项收集（socket/pid/lock/config/recent_errors）
//! 2. 目录不存在时的健壮性
//! 3. HealthStatus 三态判断逻辑（Ready/Degraded/Down）
//!
//! @author Atlas.oi
//! @date 2026-03-04

use ghostcode_daemon::diagnostics::{
    collect_diagnostics, determine_health_status, DiagnosticItem, HealthStatus, ItemStatus,
};
use std::fs;
use tempfile::TempDir;

/// 测试用例 1：collect_diagnostics 返回包含五类诊断项的报告
///
/// 前提：在临时目录构造完整的 base_dir 结构（含 pid、socket、lock、config 文件）
/// 断言：返回的 DiagnosticsReport.items 中包含 socket/pid/lock/config/recent_errors 五类
#[test]
fn collect_diagnostics_reports_five_categories() {
    let tmp = TempDir::new().expect("创建临时目录失败");
    let base_dir = tmp.path();

    // 构造 pid 文件（当前进程 PID，确保进程存活）
    let pid = std::process::id();
    fs::write(base_dir.join("ghostcoded.pid"), pid.to_string()).expect("写入 pid 文件失败");

    // 构造 socket 文件（空文件模拟占位）
    fs::write(base_dir.join("ghostcoded.sock"), "").expect("写入 socket 文件失败");

    // 构造 lock 文件
    fs::write(base_dir.join("ghostcoded.lock"), "locked").expect("写入 lock 文件失败");

    // 构造 config 文件（有效 TOML 格式）
    fs::write(
        base_dir.join("config.toml"),
        "[daemon]\nlog_level = \"info\"\n",
    )
    .expect("写入 config 文件失败");

    // 执行诊断收集
    let report = collect_diagnostics(base_dir);

    // 提取各诊断类别
    let categories: Vec<&str> = report.items.iter().map(|i| i.category.as_str()).collect();

    // 验证五类诊断项全部存在
    assert!(
        categories.contains(&"socket"),
        "缺少 socket 诊断项，实际类别: {:?}",
        categories
    );
    assert!(
        categories.contains(&"pid"),
        "缺少 pid 诊断项，实际类别: {:?}",
        categories
    );
    assert!(
        categories.contains(&"lock"),
        "缺少 lock 诊断项，实际类别: {:?}",
        categories
    );
    assert!(
        categories.contains(&"config"),
        "缺少 config 诊断项，实际类别: {:?}",
        categories
    );
    assert!(
        categories.contains(&"recent_errors"),
        "缺少 recent_errors 诊断项，实际类别: {:?}",
        categories
    );

    // 验证报告有时间戳
    assert!(!report.timestamp.is_empty(), "时间戳不应为空");
}

/// 测试用例 2：collect_diagnostics 处理不存在的目录
///
/// 前提：传入一个不存在的目录路径
/// 断言：仍返回结构化报告，各项状态为 Unknown 或 Error，不 panic
#[test]
fn collect_diagnostics_handles_missing_dir() {
    let non_existent = std::path::Path::new("/tmp/ghostcode-nonexistent-test-dir-xyzabc123");

    // 确保目录确实不存在
    let _ = fs::remove_dir_all(non_existent);

    // 不应 panic，应返回结构化报告
    let report = collect_diagnostics(non_existent);

    // 验证仍返回五类诊断项
    let categories: Vec<&str> = report.items.iter().map(|i| i.category.as_str()).collect();
    assert_eq!(
        categories.len(),
        5,
        "即使目录不存在，也应返回五类诊断项，实际: {:?}",
        categories
    );

    // 所有诊断项状态应为 Unknown 或 Error（非 Ok）
    for item in &report.items {
        assert!(
            item.status == ItemStatus::Unknown || item.status == ItemStatus::Error,
            "目录不存在时，诊断项 '{}' 状态应为 Unknown 或 Error，实际: {:?}",
            item.category,
            item.status
        );
    }
}

/// 测试用例 3：所有诊断项正常时 health_status 为 Ready
///
/// 前提：构造所有诊断项 status = Ok 的报告
/// 断言：determine_health_status 返回 HealthStatus::Ready
#[test]
fn health_status_ready_when_all_ok() {
    let items = vec![
        make_item("socket", ItemStatus::Ok),
        make_item("pid", ItemStatus::Ok),
        make_item("lock", ItemStatus::Ok),
        make_item("config", ItemStatus::Ok),
        make_item("recent_errors", ItemStatus::Ok),
    ];

    let status = determine_health_status(&items);
    assert_eq!(
        status,
        HealthStatus::Ready,
        "所有诊断项 Ok 时应返回 Ready，实际: {:?}",
        status
    );
}

/// 测试用例 4：部分诊断项失败时 health_status 为 Degraded
///
/// 前提：config 异常但 socket 正常（非关键失败）
/// 断言：determine_health_status 返回 HealthStatus::Degraded
#[test]
fn health_status_degraded_when_partial_failure() {
    let items = vec![
        make_item("socket", ItemStatus::Ok),
        make_item("pid", ItemStatus::Ok),
        make_item("lock", ItemStatus::Ok),
        make_item("config", ItemStatus::Error), // 非关键故障
        make_item("recent_errors", ItemStatus::Warning),
    ];

    let status = determine_health_status(&items);
    assert_eq!(
        status,
        HealthStatus::Degraded,
        "非关键诊断项失败时应返回 Degraded，实际: {:?}",
        status
    );
}

/// 测试用例 5：关键诊断项（socket）失败时 health_status 为 Down
///
/// 前提：socket 不可用
/// 断言：determine_health_status 返回 HealthStatus::Down
#[test]
fn health_status_down_when_critical_failure() {
    let items = vec![
        make_item("socket", ItemStatus::Error), // 关键故障
        make_item("pid", ItemStatus::Ok),
        make_item("lock", ItemStatus::Ok),
        make_item("config", ItemStatus::Ok),
        make_item("recent_errors", ItemStatus::Ok),
    ];

    let status = determine_health_status(&items);
    assert_eq!(
        status,
        HealthStatus::Down,
        "socket 诊断项 Error 时应返回 Down，实际: {:?}",
        status
    );
}

// ============================================
// 测试辅助函数
// ============================================

/// 构造测试用 DiagnosticItem
fn make_item(category: &str, status: ItemStatus) -> DiagnosticItem {
    DiagnosticItem {
        category: category.to_string(),
        status,
        message: format!("{} 诊断测试", category),
        details: None,
    }
}
