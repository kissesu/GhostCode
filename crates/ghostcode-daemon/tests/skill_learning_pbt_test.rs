//! Skill Learning PBT 属性测试
//!
//! 验证去重幂等性、质量分约束等不变量
//!
//! @author Atlas.oi
//! @date 2026-03-03

use ghostcode_daemon::skill_learning::{ingest_session_fragment, SessionFragment, SkillStore};
use proptest::prelude::*;

fn make_store() -> SkillStore {
    SkillStore::new()
}

proptest! {
    /// 质量分约束：confidence 参数在任何值下，候选的 confidence 字段值均在 [70, 100]
    #[test]
    fn confidence_always_bounded(conf in 70u8..=100u8) {
        let mut store = make_store();
        let fragment = SessionFragment {
            problem: "P".to_string(),
            solution: "S".to_string(),
            confidence: conf,
            context: "c".to_string(),
            suggested_triggers: vec![],
            suggested_tags: vec![],
        };
        if let Some(candidate) = ingest_session_fragment(&mut store, fragment) {
            prop_assert!(candidate.confidence <= 100, "confidence 应 <= 100");
            prop_assert!(candidate.confidence >= 70, "候选 confidence 应 >= 70");
        }
    }

    /// Hash 去重幂等性：相同 (problem, solution) 多次 ingest -> 候选列表只有一条
    #[test]
    fn dedup_idempotent(n in 2usize..10) {
        let mut store = make_store();
        for _ in 0..n {
            let fragment = SessionFragment {
                problem: "相同问题".to_string(),
                solution: "相同解答".to_string(),
                confidence: 80,
                context: "ctx".to_string(),
                suggested_triggers: vec![],
                suggested_tags: vec![],
            };
            ingest_session_fragment(&mut store, fragment);
        }
        prop_assert_eq!(store.candidates().len(), 1, "相同内容应只保留一条候选");
    }

    /// 低置信度内容绝不进入候选列表
    #[test]
    fn low_confidence_never_stored(conf in 0u8..70u8) {
        let mut store = make_store();
        let fragment = SessionFragment {
            problem: "P".to_string(),
            solution: "S".to_string(),
            confidence: conf,
            context: "c".to_string(),
            suggested_triggers: vec![],
            suggested_tags: vec![],
        };
        ingest_session_fragment(&mut store, fragment);
        prop_assert_eq!(store.candidates().len(), 0, "低置信度内容不应进入候选列表");
    }
}
