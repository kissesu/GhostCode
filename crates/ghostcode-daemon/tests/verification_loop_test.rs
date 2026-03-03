// @file verification_loop_test.rs
// @description Ralph 验证循环行为测试
//              测试最大迭代限制、失败原因保留、终态不可再推进等行为
// @author Atlas.oi
// @date 2026-03-03

use ghostcode_daemon::verification::{
    CheckStatus, RunStatus, VerificationCheckKind, VerificationStateStore, VerifyEvent,
};

// 辅助函数：向 store 注入一轮全部失败的检查
fn inject_all_failed_round(store: &mut VerificationStateStore, group_id: &str, run_id: &str) {
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
            .apply_event(
                group_id,
                run_id,
                VerifyEvent::CheckFailed(kind, "检查失败".to_string()),
            )
            .unwrap();
    }
    store
        .apply_event(group_id, run_id, VerifyEvent::AdvanceIteration)
        .unwrap();
}

// 辅助函数：向 store 注入一轮全部通过的检查（不推进迭代）
fn inject_all_passed_round(store: &mut VerificationStateStore, group_id: &str, run_id: &str) {
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
            .apply_event(group_id, run_id, VerifyEvent::CheckPassed(kind))
            .unwrap();
    }
}

// 测试达到 max_iterations (10) 后 → verdict: Rejected
#[test]
fn max_iterations_reached_yields_rejected() {
    let mut store = VerificationStateStore::new();
    store
        .start_run("group1".to_string(), "run1".to_string())
        .unwrap();

    // 循环执行 10 轮全部失败的检查
    for _ in 0..10 {
        inject_all_failed_round(&mut store, "group1", "run1");
    }

    let state = store.get_run("group1", "run1").unwrap();
    // 达到最大迭代次数后应被 Rejected
    assert!(matches!(state.status, RunStatus::Rejected));
}

// 测试每轮保留失败原因（failure_reasons 列表）
#[test]
fn failure_reasons_preserved_per_iteration() {
    let mut store = VerificationStateStore::new();
    store
        .start_run("group1".to_string(), "run1".to_string())
        .unwrap();

    // 第一轮：Build 和 Test 失败
    store
        .apply_event(
            "group1",
            "run1",
            VerifyEvent::CheckFailed(
                VerificationCheckKind::Build,
                "编译错误：缺少分号".to_string(),
            ),
        )
        .unwrap();
    store
        .apply_event(
            "group1",
            "run1",
            VerifyEvent::CheckFailed(
                VerificationCheckKind::Test,
                "测试失败：断言不匹配".to_string(),
            ),
        )
        .unwrap();
    // 其余通过
    let other_kinds = vec![
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
    store
        .apply_event("group1", "run1", VerifyEvent::AdvanceIteration)
        .unwrap();

    let state = store.get_run("group1", "run1").unwrap();

    // 历史记录应有 1 条
    assert_eq!(state.history.len(), 1);

    // 第一轮历史应包含 2 个失败原因
    let first_iter = &state.history[0];
    assert_eq!(first_iter.failure_reasons.len(), 2);
    assert!(first_iter
        .failure_reasons
        .iter()
        .any(|r| r.contains("编译错误")));
    assert!(first_iter
        .failure_reasons
        .iter()
        .any(|r| r.contains("测试失败")));
}

// 测试终态（Approved）后不可再 advance
#[test]
fn terminal_approved_cannot_advance() {
    let mut store = VerificationStateStore::new();
    store
        .start_run("group1".to_string(), "run1".to_string())
        .unwrap();

    // 全部通过，推进到 Approved
    inject_all_passed_round(&mut store, "group1", "run1");
    store
        .apply_event("group1", "run1", VerifyEvent::AdvanceIteration)
        .unwrap();

    // 确认已经是 Approved
    let state = store.get_run("group1", "run1").unwrap();
    assert!(matches!(state.status, RunStatus::Approved));

    // 尝试再次推进，应该返回错误
    let result = store.apply_event("group1", "run1", VerifyEvent::AdvanceIteration);
    assert!(result.is_err());
}

// 测试终态（Rejected）后不可再 advance
#[test]
fn terminal_rejected_cannot_advance() {
    let mut store = VerificationStateStore::new();
    store
        .start_run("group1".to_string(), "run1".to_string())
        .unwrap();

    // 执行 10 轮失败，达到 Rejected
    for _ in 0..10 {
        inject_all_failed_round(&mut store, "group1", "run1");
    }

    let state = store.get_run("group1", "run1").unwrap();
    assert!(matches!(state.status, RunStatus::Rejected));

    // 尝试再次推进，应该返回错误
    let result = store.apply_event("group1", "run1", VerifyEvent::AdvanceIteration);
    assert!(result.is_err());
}

// 测试终态（Cancelled）后不可再 advance
#[test]
fn terminal_cancelled_cannot_advance() {
    let mut store = VerificationStateStore::new();
    store
        .start_run("group1".to_string(), "run1".to_string())
        .unwrap();

    // 取消运行
    store
        .apply_event("group1", "run1", VerifyEvent::Cancel)
        .unwrap();

    let state = store.get_run("group1", "run1").unwrap();
    assert!(matches!(state.status, RunStatus::Cancelled));

    // 尝试再次推进，应该返回错误
    let result = store.apply_event("group1", "run1", VerifyEvent::AdvanceIteration);
    assert!(result.is_err());
}

// 测试相同 (group_id, run_id) 不能重复启动
#[test]
fn cannot_start_same_run_twice() {
    let mut store = VerificationStateStore::new();
    store
        .start_run("group1".to_string(), "run1".to_string())
        .unwrap();

    // 第二次启动同一个 run 应该返回错误
    let result = store.start_run("group1".to_string(), "run1".to_string());
    assert!(result.is_err());
}

// 测试获取不存在的运行状态返回 None
#[test]
fn get_nonexistent_run_returns_none() {
    let store = VerificationStateStore::new();
    let result = store.get_run("nonexistent", "run1");
    assert!(result.is_none());
}

// 测试 CheckStatus 类型的基本访问
#[test]
fn check_status_variants_accessible() {
    let pending = CheckStatus::Pending;
    let passed = CheckStatus::Passed;
    let failed = CheckStatus::Failed("错误消息".to_string());

    assert!(matches!(pending, CheckStatus::Pending));
    assert!(matches!(passed, CheckStatus::Passed));
    assert!(matches!(failed, CheckStatus::Failed(_)));
}
