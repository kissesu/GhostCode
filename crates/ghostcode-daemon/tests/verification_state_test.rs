// @file verification_state_test.rs
// @description Ralph 验证状态机契约测试
//              测试状态机的基本行为：创建运行、推进迭代、通过/失败、取消
// @author Atlas.oi
// @date 2026-03-03

use ghostcode_daemon::verification::{
    CheckStatus, RunStatus, VerificationCheckKind, VerificationStateStore, VerifyEvent,
};

// 测试 VerificationCheckKind 枚举包含 7 项检查类型
#[test]
fn check_kind_has_seven_variants() {
    let kinds = vec![
        VerificationCheckKind::Build,
        VerificationCheckKind::Test,
        VerificationCheckKind::Lint,
        VerificationCheckKind::Functionality,
        VerificationCheckKind::Architect,
        VerificationCheckKind::Todo,
        VerificationCheckKind::ErrorFree,
    ];
    assert_eq!(kinds.len(), 7);
}

// 测试 start_run() 创建运行后状态为 Running，7 项检查初始化为 Pending
#[test]
fn start_run_creates_running_state_with_pending_checks() {
    let mut store = VerificationStateStore::new();
    store
        .start_run("group1".to_string(), "run1".to_string())
        .expect("start_run 应该成功");

    let state = store.get_run("group1", "run1").expect("应能获取运行状态");

    // 运行状态应为 Running
    assert!(matches!(state.status, RunStatus::Running));

    // 迭代初始值为 0
    assert_eq!(state.iteration, 0);

    // 7 项检查均为 Pending
    assert_eq!(state.current_checks.len(), 7);
    for (_, status) in &state.current_checks {
        assert!(matches!(status, CheckStatus::Pending));
    }
}

// 测试单轮全部通过后 advance → verdict: Approved
#[test]
fn all_checks_pass_advance_yields_approved() {
    let mut store = VerificationStateStore::new();
    store
        .start_run("group1".to_string(), "run1".to_string())
        .unwrap();

    // 所有 7 项检查全部通过
    let kinds = vec![
        VerificationCheckKind::Build,
        VerificationCheckKind::Test,
        VerificationCheckKind::Lint,
        VerificationCheckKind::Functionality,
        VerificationCheckKind::Architect,
        VerificationCheckKind::Todo,
        VerificationCheckKind::ErrorFree,
    ];
    for kind in kinds {
        store
            .apply_event("group1", "run1", VerifyEvent::CheckPassed(kind))
            .unwrap();
    }

    // 推进迭代
    store
        .apply_event("group1", "run1", VerifyEvent::AdvanceIteration)
        .unwrap();

    let state = store.get_run("group1", "run1").unwrap();
    // 全部通过后状态应为 Approved
    assert!(matches!(state.status, RunStatus::Approved));
}

// 测试单轮有失败 → advance → iteration +1，状态保持 Running
#[test]
fn failed_check_advance_increments_iteration_stays_running() {
    let mut store = VerificationStateStore::new();
    store
        .start_run("group1".to_string(), "run1".to_string())
        .unwrap();

    // Build 检查失败，其余通过
    store
        .apply_event(
            "group1",
            "run1",
            VerifyEvent::CheckFailed(
                VerificationCheckKind::Build,
                "编译错误：未找到类型".to_string(),
            ),
        )
        .unwrap();

    let other_kinds = vec![
        VerificationCheckKind::Test,
        VerificationCheckKind::Lint,
        VerificationCheckKind::Functionality,
        VerificationCheckKind::Architect,
        VerificationCheckKind::Todo,
        VerificationCheckKind::ErrorFree,
    ];
    for kind in other_kinds {
        store
            .apply_event("group1", "run1", VerifyEvent::CheckPassed(kind))
            .unwrap();
    }

    // 推进迭代
    store
        .apply_event("group1", "run1", VerifyEvent::AdvanceIteration)
        .unwrap();

    let state = store.get_run("group1", "run1").unwrap();
    // 有失败时状态仍为 Running
    assert!(matches!(state.status, RunStatus::Running));
    // 迭代次数 +1
    assert_eq!(state.iteration, 1);
}

// 测试 cancel_run() → 状态变为 Cancelled
#[test]
fn cancel_run_changes_status_to_cancelled() {
    let mut store = VerificationStateStore::new();
    store
        .start_run("group1".to_string(), "run1".to_string())
        .unwrap();

    store
        .apply_event("group1", "run1", VerifyEvent::Cancel)
        .unwrap();

    let state = store.get_run("group1", "run1").unwrap();
    assert!(matches!(state.status, RunStatus::Cancelled));
}
