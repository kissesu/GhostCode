//! Skill Learning 核心引擎单元测试
//!
//! 测试 ingest_session_fragment / list_skill_candidates / promote_skill 函数
//!
//! @author Atlas.oi
//! @date 2026-03-03

use ghostcode_daemon::skill_learning::{
    ingest_session_fragment, list_skill_candidates, promote_skill, SessionFragment, SkillStore,
};

fn make_store() -> SkillStore {
    SkillStore::new()
}

fn make_fragment(problem: &str, solution: &str, confidence: u8) -> SessionFragment {
    SessionFragment {
        problem: problem.to_string(),
        solution: solution.to_string(),
        confidence,
        context: "test context".to_string(),
        suggested_triggers: vec!["test".to_string()],
        suggested_tags: vec!["rust".to_string()],
    }
}

#[test]
fn ingest_high_confidence_creates_candidate() {
    let mut store = make_store();
    let fragment = make_fragment("修复 Rust 生命周期错误", "添加显式生命周期标注", 75);
    let result = ingest_session_fragment(&mut store, fragment);
    assert!(result.is_some(), "置信度 >= 70 应创建候选");
    assert_eq!(store.candidates().len(), 1);
}

#[test]
fn ingest_low_confidence_ignored() {
    let mut store = make_store();
    let fragment = make_fragment("简单问题", "简单解答", 50);
    let result = ingest_session_fragment(&mut store, fragment);
    assert!(result.is_none(), "置信度 < 70 应被忽略");
    assert_eq!(store.candidates().len(), 0);
}

#[test]
fn ingest_same_content_no_duplicate() {
    let mut store = make_store();
    let f1 = make_fragment("问题A", "解答A", 80);
    let f2 = make_fragment("问题A", "解答A", 85);
    ingest_session_fragment(&mut store, f1);
    ingest_session_fragment(&mut store, f2);
    // 相同 (problem, solution) 组合应去重
    assert_eq!(store.candidates().len(), 1, "相同内容应去重");
}

#[test]
fn list_candidates_returns_all_above_threshold() {
    let mut store = make_store();
    ingest_session_fragment(&mut store, make_fragment("P1", "S1", 80));
    ingest_session_fragment(&mut store, make_fragment("P2", "S2", 90));
    ingest_session_fragment(&mut store, make_fragment("P3", "S3", 40)); // 低于阈值
    let candidates = list_skill_candidates(&store);
    assert_eq!(candidates.len(), 2);
}

#[test]
fn promote_skill_removes_from_candidates() {
    let mut store = make_store();
    let fragment = make_fragment("可复用的解法", "通用解决方案", 85);
    let candidate = ingest_session_fragment(&mut store, fragment).unwrap();
    let candidate_id = candidate.id.clone();

    let skill = promote_skill(&mut store, &candidate_id, "fix-pattern", "修复模式");
    assert!(skill.is_ok(), "promote 应成功");
    // 候选列表中应移除已 promote 的候选
    assert!(
        !store.candidates().iter().any(|c| c.id == candidate_id),
        "promote 后候选应移除"
    );
}

#[test]
fn promote_nonexistent_returns_error() {
    let mut store = make_store();
    let result = promote_skill(&mut store, "nonexistent-id", "test", "Test");
    assert!(result.is_err(), "promote 不存在的候选应返回错误");
}

#[test]
fn promoted_skills_quality_capped_at_100() {
    let mut store = make_store();
    let fragment = make_fragment("P", "S", 100);
    let candidate = ingest_session_fragment(&mut store, fragment).unwrap();
    let skill = promote_skill(&mut store, &candidate.id, "cap-test", "Cap Test").unwrap();
    assert!(skill.metadata.quality <= 100, "quality 不应超过 100");
}
