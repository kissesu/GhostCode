// @file process_recovery_test.rs
// @description Daemon 僵尸进程清理与异常恢复功能测试
//
// 测试覆盖：
// 1. reap_orphan_processes - 孤儿进程 pid/socket 文件清理
// 2. handle_abnormal_exit - Actor 异常退出恢复动作判断
// 3. stop_actor 幂等性 - 连续两次 stop 不留孤儿记录
//
// @author Atlas.oi
// @date 2026-03-04

use ghostcode_daemon::recovery::{handle_abnormal_exit, reap_orphan_processes, RecoveryAction};
use std::fs;

// ============================================
// 测试用例 1：清理 stale pid 和 socket 文件
// stale PID = 不对应任何运行中进程的 PID（使用极大值）
// 期望：pid 文件 和对应 socket 文件均被清理
// ============================================
#[test]
fn reap_orphan_processes_cleans_stale_pid_and_socket() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let base = tmp_dir.path();

    // 创建 stale pid 文件（使用 PID=999999999，几乎不可能存在）
    let stale_pid: u32 = 999_999_999;
    let pid_file = base.join("ghostcoded.pid");
    let sock_file = base.join("ghostcoded.sock");

    fs::write(&pid_file, stale_pid.to_string()).unwrap();
    fs::write(&sock_file, b"dummy socket").unwrap();

    // 执行清理
    let cleaned = reap_orphan_processes(base).unwrap();

    // 验证文件被清理
    assert!(
        !pid_file.exists(),
        "stale pid 文件应被清理，但仍存在"
    );
    assert!(
        !sock_file.exists(),
        "stale socket 文件应被清理，但仍存在"
    );
    // 返回已清理的记录
    assert!(!cleaned.is_empty(), "应返回至少一条清理记录");
}

// ============================================
// 测试用例 2：保留活跃进程的 pid 文件
// 使用当前进程 PID（std::process::id()），进程必然存在
// 期望：pid 文件被保留（不清理活跃进程）
// ============================================
#[test]
fn reap_preserves_running_process_artifacts() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let base = tmp_dir.path();

    // 使用当前进程 PID（进程存在）
    let current_pid = std::process::id();
    let pid_file = base.join("ghostcoded.pid");
    let sock_file = base.join("ghostcoded.sock");

    fs::write(&pid_file, current_pid.to_string()).unwrap();
    fs::write(&sock_file, b"active socket").unwrap();

    // 执行清理
    let cleaned = reap_orphan_processes(base).unwrap();

    // 验证：活跃进程的文件不被清理
    assert!(
        pid_file.exists(),
        "活跃进程的 pid 文件不应被清理，但被删除了"
    );
    // 活跃进程无孤儿记录
    assert!(
        cleaned.is_empty(),
        "活跃进程不应产生清理记录，但 cleaned={:?}",
        cleaned
    );
}

// ============================================
// 测试用例 3：Actor 异常退出恢复动作判断
// 模拟各种 exit code 和 signal 场景
// ============================================
#[test]
fn handle_abnormal_exit_returns_restart_for_transient_errors() {
    // exit code = 1 → 临时错误，应重启
    let action = handle_abnormal_exit("actor-001", Some(1), None);
    assert_eq!(
        action,
        RecoveryAction::Restart,
        "exit code=1 应返回 Restart，但得到 {:?}",
        action
    );
}

#[test]
fn handle_abnormal_exit_returns_mark_failed_for_fatal_errors() {
    // exit code > 1 → 配置/代码错误，标记失败
    let action = handle_abnormal_exit("actor-002", Some(127), None);
    assert!(
        matches!(action, RecoveryAction::MarkFailed { .. }),
        "exit code=127 应返回 MarkFailed，但得到 {:?}",
        action
    );
}

#[test]
fn handle_abnormal_exit_returns_restart_for_signal_termination() {
    // 信号终止（SIGKILL=9）→ 应重启
    let action = handle_abnormal_exit("actor-003", None, Some(9));
    assert_eq!(
        action,
        RecoveryAction::Restart,
        "SIGKILL 终止应返回 Restart，但得到 {:?}",
        action
    );
}

// ============================================
// 测试用例 4：连续两次 stop 不留孤儿记录
// 验证清理操作幂等：第二次不报错，不留孤儿记录
// ============================================
#[test]
fn repeated_stop_leaves_no_orphan_records() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let base = tmp_dir.path();

    // 创建 stale pid 文件
    let stale_pid: u32 = 999_999_998;
    let pid_file = base.join("ghostcoded.pid");
    let sock_file = base.join("ghostcoded.sock");

    fs::write(&pid_file, stale_pid.to_string()).unwrap();
    fs::write(&sock_file, b"dummy").unwrap();

    // 第一次清理
    let first = reap_orphan_processes(base).unwrap();
    assert!(!first.is_empty(), "第一次清理应有记录");

    // 第二次清理（文件已不存在）
    let second = reap_orphan_processes(base);
    assert!(
        second.is_ok(),
        "第二次清理不应报错，但得到 {:?}",
        second
    );
    let second_cleaned = second.unwrap();
    assert!(
        second_cleaned.is_empty(),
        "第二次清理不应有记录（已无孤儿文件），但 cleaned={:?}",
        second_cleaned
    );

    // 确认无孤儿记录残留
    assert!(!pid_file.exists(), "pid 文件应被清理");
    assert!(!sock_file.exists(), "socket 文件应被清理");
}
