//! Phase 4 dispatch op 注册验证测试
//!
//! 验证所有 Phase 4 新增的 op 字符串已正确注册到 KNOWN_OPS
//!
//! @author Atlas.oi
//! @date 2026-03-03

use ghostcode_daemon::dispatch::KNOWN_OPS;

/// Phase 4 新增的所有 op 字符串
const PHASE4_OPS: &[&str] = &[
    "dashboard_snapshot",
    "dashboard_timeline",
    "dashboard_agents",
    "skill_extract",
    "skill_list",
    "skill_promote",
    "skill_learn_fragment",
    "team_skill_list",
];

#[test]
fn all_phase4_ops_registered() {
    for op in PHASE4_OPS {
        assert!(
            KNOWN_OPS.contains(op),
            "Phase 4 op '{}' 未注册到 KNOWN_OPS",
            op
        );
    }
}

#[test]
fn known_ops_no_duplicates() {
    let mut seen = std::collections::HashSet::new();
    for op in KNOWN_OPS {
        assert!(seen.insert(*op), "KNOWN_OPS 中存在重复 op: '{}'", op);
    }
}
