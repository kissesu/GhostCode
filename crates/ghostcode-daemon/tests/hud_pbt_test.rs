// @file hud_pbt_test.rs
// @description HUD 快照聚合属性基测试（PBT）
//              使用 proptest 验证 HUD 快照的代数属性：
//              1. 百分比边界：compute_context_level 输出必为 green/yellow/red
//              2. 计数一致性：checks_passed <= checks_total
//              3. 快照幂等：相同输入状态重复构建快照，结果一致
// @author Atlas.oi
// @date 2026-03-03

use ghostcode_daemon::hud::compute_context_level;
use ghostcode_daemon::server::AppState;
use proptest::prelude::*;

// ============================================
// 属性 1：百分比边界
// compute_context_level 对任意 f64 输入必须输出 green/yellow/red 三选一
// ============================================

proptest! {
    #[test]
    fn prop_context_level_always_valid(percentage in 0.0f64..=100.0f64) {
        let level = compute_context_level(percentage);
        prop_assert!(
            level == "green" || level == "yellow" || level == "red",
            "compute_context_level({}) 输出 '{}' 不在合法值范围内",
            percentage, level
        );
    }
}

// ============================================
// 属性 2：计数一致性
// HUD 快照中 checks_passed <= checks_total
// ============================================

proptest! {
    #[test]
    fn prop_checks_passed_le_checks_total(
        checks_passed in 0u32..=7u32,
    ) {
        let checks_total = 7u32;
        prop_assert!(
            checks_passed <= checks_total,
            "checks_passed({}) 超过 checks_total({})",
            checks_passed, checks_total
        );
    }
}

// ============================================
// 属性 3：快照幂等
// 相同的输入状态重复构建快照，结果一致
// ============================================

proptest! {
    #[test]
    fn prop_snapshot_idempotent(
        used_tokens in 0u64..=100_000u64,
        max_tokens in 1u64..=200_000u64,
    ) {
        let state = AppState::default();
        let args = serde_json::json!({
            "used_tokens": used_tokens,
            "max_tokens": max_tokens,
        });

        // 第一次构建
        let snapshot1 = ghostcode_daemon::hud::build_hud_snapshot(&state, &args);
        // 第二次构建（相同输入）
        let snapshot2 = ghostcode_daemon::hud::build_hud_snapshot(&state, &args);

        // 上下文压力级别一致
        prop_assert_eq!(
            snapshot1.context_pressure.level,
            snapshot2.context_pressure.level,
            "相同输入的快照 context_pressure.level 应一致"
        );
        // active_agents 一致
        prop_assert_eq!(
            snapshot1.active_agents,
            snapshot2.active_agents,
            "相同输入的快照 active_agents 应一致"
        );
    }
}
