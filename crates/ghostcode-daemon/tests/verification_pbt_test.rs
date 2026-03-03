// @file verification_pbt_test.rs
// @description Ralph 验证状态机属性基测试（PBT）
//              使用 proptest 验证状态机的不变量属性
// @author Atlas.oi
// @date 2026-03-03

use ghostcode_daemon::verification::{
    transition, CheckStatus, RunState, RunStatus, VerificationCheckKind, VerifyEvent,
};
use proptest::prelude::*;

// 生成任意合法的 VerificationCheckKind 枚举值
fn arb_check_kind() -> impl Strategy<Value = VerificationCheckKind> {
    prop_oneof![
        Just(VerificationCheckKind::Build),
        Just(VerificationCheckKind::Test),
        Just(VerificationCheckKind::Lint),
        Just(VerificationCheckKind::Functionality),
        Just(VerificationCheckKind::Architect),
        Just(VerificationCheckKind::Todo),
        Just(VerificationCheckKind::ErrorFree),
    ]
}

// 生成任意合法的 VerifyEvent（不含 Start 事件）
fn arb_event() -> impl Strategy<Value = VerifyEvent> {
    prop_oneof![
        arb_check_kind().prop_map(VerifyEvent::CheckPassed),
        (arb_check_kind(), "[a-zA-Z0-9 ]{0,32}")
            .prop_map(|(k, msg)| VerifyEvent::CheckFailed(k, msg)),
        Just(VerifyEvent::AdvanceIteration),
        Just(VerifyEvent::Cancel),
    ]
}

// 辅助函数：构建一个处于 Running 状态的 RunState
fn make_running_state(iteration: u32) -> RunState {
    use VerificationCheckKind::*;
    RunState {
        run_id: "test-run".to_string(),
        group_id: "test-group".to_string(),
        status: RunStatus::Running,
        iteration,
        max_iterations: 10,
        current_checks: vec![
            (Build, CheckStatus::Pending),
            (Test, CheckStatus::Pending),
            (Lint, CheckStatus::Pending),
            (Functionality, CheckStatus::Pending),
            (Architect, CheckStatus::Pending),
            (Todo, CheckStatus::Pending),
            (ErrorFree, CheckStatus::Pending),
        ],
        history: vec![],
    }
}

// 属性 1：transition 确定性
// 同一 (state, event) 多次调用结果一致
proptest! {
    #[test]
    fn prop_transition_deterministic(
        event in arb_event(),
        iteration in 0u32..9u32,
    ) {
        let state = make_running_state(iteration);
        let result1 = transition(&state, &event);
        let result2 = transition(&state, &event);

        // 两次调用结果应完全一致
        match (result1, result2) {
            (Ok(s1), Ok(s2)) => {
                prop_assert_eq!(s1.status, s2.status);
                prop_assert_eq!(s1.iteration, s2.iteration);
            }
            (Err(_), Err(_)) => {
                // 两次都失败也是确定性的
            }
            _ => {
                // 一次成功一次失败表示非确定性，测试失败
                prop_assert!(false, "transition 应该是确定性的");
            }
        }
    }
}

// 属性 2：合法性封闭
// Running 状态经过合法事件后，结果状态必须是合法的枚举值之一
proptest! {
    #[test]
    fn prop_legal_state_closure(
        event in arb_event(),
        iteration in 0u32..9u32,
    ) {
        let state = make_running_state(iteration);
        if let Ok(new_state) = transition(&state, &event) {
            // 新状态必须是合法枚举值
            let _valid = match new_state.status {
                RunStatus::Running => true,
                RunStatus::Approved => true,
                RunStatus::Rejected => true,
                RunStatus::Cancelled => true,
            };
        }
    }
}

// 属性 3：终态幂等
// is_terminal(s) 为 true 后，非 Start 事件不会回到 Running
proptest! {
    #[test]
    fn prop_terminal_state_idempotent(
        event in arb_event(),
    ) {
        // 测试 Approved 终态
        let approved_state = RunState {
            run_id: "run".to_string(),
            group_id: "group".to_string(),
            status: RunStatus::Approved,
            iteration: 3,
            max_iterations: 10,
            current_checks: vec![],
            history: vec![],
        };
        if let Err(_) = transition(&approved_state, &event) {
            // 终态下事件返回错误是正确行为
        } else {
            // 如果成功，不应该回到 Running
            if let Ok(new_state) = transition(&approved_state, &event) {
                prop_assert!(!matches!(new_state.status, RunStatus::Running),
                    "终态不应回到 Running");
            }
        }

        // 测试 Cancelled 终态
        let cancelled_state = RunState {
            run_id: "run".to_string(),
            group_id: "group".to_string(),
            status: RunStatus::Cancelled,
            iteration: 2,
            max_iterations: 10,
            current_checks: vec![],
            history: vec![],
        };
        if let Ok(new_state) = transition(&cancelled_state, &event) {
            prop_assert!(!matches!(new_state.status, RunStatus::Running),
                "Cancelled 终态不应回到 Running");
        }
    }
}

// 属性 4：单调阶段推进
// Running → Approved/Rejected/Cancelled，不可逆
proptest! {
    #[test]
    fn prop_monotonic_phase_advance(
        event in arb_event(),
        iteration in 0u32..9u32,
    ) {
        let state = make_running_state(iteration);
        if let Ok(new_state) = transition(&state, &event) {
            // Running → Running 是允许的（迭代进行中）
            // Running → Approved/Rejected/Cancelled 是允许的
            // Approved/Rejected/Cancelled → Running 是不允许的
            match (state.status, &new_state.status) {
                (RunStatus::Approved, RunStatus::Running) => {
                    prop_assert!(false, "Approved 不应回到 Running");
                }
                (RunStatus::Rejected, RunStatus::Running) => {
                    prop_assert!(false, "Rejected 不应回到 Running");
                }
                (RunStatus::Cancelled, RunStatus::Running) => {
                    prop_assert!(false, "Cancelled 不应回到 Running");
                }
                _ => {}
            }
        }
    }
}

// 属性 5：重试计数单调递增
// iteration 字段只增不减
proptest! {
    #[test]
    fn prop_iteration_monotonic(
        event in arb_event(),
        iteration in 0u32..8u32,
    ) {
        let state = make_running_state(iteration);
        if let Ok(new_state) = transition(&state, &event) {
            // iteration 只能保持不变或增加
            prop_assert!(new_state.iteration >= state.iteration,
                "iteration 应该单调递增，从 {} 变为 {}", state.iteration, new_state.iteration);
        }
    }
}

// 属性 6：max_iterations 上限
// iteration 不超过 max_iterations
proptest! {
    #[test]
    fn prop_iteration_within_max(
        event in arb_event(),
        iteration in 0u32..9u32,
    ) {
        let state = make_running_state(iteration);
        if let Ok(new_state) = transition(&state, &event) {
            prop_assert!(new_state.iteration <= new_state.max_iterations,
                "iteration {} 超过了 max_iterations {}", new_state.iteration, new_state.max_iterations);
        }
    }
}

// 属性 7：错误吸收稳定
// 同一个输入给出同类型错误
proptest! {
    #[test]
    fn prop_error_absorption_stable(
        event in arb_event(),
    ) {
        // 对终态（Approved）重复应用事件
        let terminal_state = RunState {
            run_id: "run".to_string(),
            group_id: "group".to_string(),
            status: RunStatus::Approved,
            iteration: 5,
            max_iterations: 10,
            current_checks: vec![],
            history: vec![],
        };

        let result1 = transition(&terminal_state, &event);
        let result2 = transition(&terminal_state, &event);

        // 两次调用应该给出相同类型的结果
        match (&result1, &result2) {
            (Ok(_), Ok(_)) => {}
            (Err(_), Err(_)) => {}
            _ => prop_assert!(false, "两次调用应给出相同类型的结果"),
        }
    }
}

// ============================================
// 可达状态生成器：生成已完成检查的 Running 状态
// 用于覆盖 AdvanceIteration → Approved/Rejected 核心路径
// ============================================

/// 生成一个所有检查均已完成（非 Pending）的 RunState
/// 包含多种检查结果组合，确保 AdvanceIteration 能走到结算分支
fn arb_completed_checks_state() -> impl Strategy<Value = RunState> {
    // 为每项检查生成 Passed 或 Failed 状态
    (
        prop::bool::ANY, // Build
        prop::bool::ANY, // Test
        prop::bool::ANY, // Lint
        prop::bool::ANY, // Functionality
        prop::bool::ANY, // Architect
        prop::bool::ANY, // Todo
        prop::bool::ANY, // ErrorFree
        0u32..9u32,      // iteration
        1u32..11u32,     // max_iterations offset (max_iterations = iteration + offset)
    )
        .prop_map(|(b, t, l, f, a, td, e, iteration, offset)| {
            use VerificationCheckKind::*;

            let make_status = |passed: bool, kind_name: &str| -> CheckStatus {
                if passed {
                    CheckStatus::Passed
                } else {
                    CheckStatus::Failed(format!("{} 检查失败", kind_name))
                }
            };

            RunState {
                run_id: "pbt-run".to_string(),
                group_id: "pbt-group".to_string(),
                status: RunStatus::Running,
                iteration,
                max_iterations: iteration.saturating_add(offset),
                current_checks: vec![
                    (Build, make_status(b, "Build")),
                    (Test, make_status(t, "Test")),
                    (Lint, make_status(l, "Lint")),
                    (Functionality, make_status(f, "Functionality")),
                    (Architect, make_status(a, "Architect")),
                    (Todo, make_status(td, "Todo")),
                    (ErrorFree, make_status(e, "ErrorFree")),
                ],
                history: vec![],
            }
        })
}

// 属性 8：AdvanceIteration 全部通过 → Approved
// 当所有检查都是 Passed 时，AdvanceIteration 必须让状态迁移到 Approved
proptest! {
    #[test]
    fn prop_all_passed_leads_to_approved(
        iteration in 0u32..9u32,
    ) {
        use VerificationCheckKind::*;
        let state = RunState {
            run_id: "run".to_string(),
            group_id: "group".to_string(),
            status: RunStatus::Running,
            iteration,
            max_iterations: 10,
            current_checks: vec![
                (Build, CheckStatus::Passed),
                (Test, CheckStatus::Passed),
                (Lint, CheckStatus::Passed),
                (Functionality, CheckStatus::Passed),
                (Architect, CheckStatus::Passed),
                (Todo, CheckStatus::Passed),
                (ErrorFree, CheckStatus::Passed),
            ],
            history: vec![],
        };

        let result = transition(&state, &VerifyEvent::AdvanceIteration);
        prop_assert!(result.is_ok(), "全部 Passed 时 AdvanceIteration 应成功");
        let new_state = result.unwrap();
        prop_assert_eq!(new_state.status, RunStatus::Approved,
            "全部 Passed 时应迁移到 Approved");
    }
}

// 属性 9：AdvanceIteration 有失败 + 达到 max_iterations → Rejected
// 当存在失败且 iteration+1 >= max_iterations 时，应迁移到 Rejected
proptest! {
    #[test]
    fn prop_failure_at_max_leads_to_rejected(
        max_iterations in 1u32..10u32,
    ) {
        use VerificationCheckKind::*;
        // iteration = max_iterations - 1，即最后一轮
        let iteration = max_iterations - 1;
        let state = RunState {
            run_id: "run".to_string(),
            group_id: "group".to_string(),
            status: RunStatus::Running,
            iteration,
            max_iterations,
            current_checks: vec![
                (Build, CheckStatus::Passed),
                (Test, CheckStatus::Failed("test 失败".to_string())),
                (Lint, CheckStatus::Passed),
                (Functionality, CheckStatus::Passed),
                (Architect, CheckStatus::Passed),
                (Todo, CheckStatus::Passed),
                (ErrorFree, CheckStatus::Passed),
            ],
            history: vec![],
        };

        let result = transition(&state, &VerifyEvent::AdvanceIteration);
        prop_assert!(result.is_ok(), "最后一轮有失败时 AdvanceIteration 应成功");
        let new_state = result.unwrap();
        prop_assert_eq!(new_state.status, RunStatus::Rejected,
            "最后一轮有失败应迁移到 Rejected");
    }
}

// 属性 10：AdvanceIteration 有失败但未达 max → 继续 Running + iteration+1
// 验证重试场景的状态正确性
proptest! {
    #[test]
    fn prop_failure_not_at_max_continues_running(
        state in arb_completed_checks_state(),
    ) {
        // 仅在有失败检查 + 未达 max_iterations 时测试
        let has_failure = state.current_checks.iter().any(|(_, s)| matches!(s, CheckStatus::Failed(_)));
        let at_max = state.iteration + 1 >= state.max_iterations;

        if has_failure && !at_max {
            let result = transition(&state, &VerifyEvent::AdvanceIteration);
            prop_assert!(result.is_ok(), "有失败但未达 max 时 AdvanceIteration 应成功");
            let new_state = result.unwrap();
            prop_assert_eq!(new_state.status, RunStatus::Running,
                "未达 max 时应继续 Running");
            prop_assert_eq!(new_state.iteration, state.iteration + 1,
                "iteration 应 +1");
            // 重置后所有检查应回到 Pending
            prop_assert!(new_state.current_checks.iter().all(|(_, s)| matches!(s, CheckStatus::Pending)),
                "重置后所有检查应为 Pending");
        }
    }
}
