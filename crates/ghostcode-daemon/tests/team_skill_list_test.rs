//! team_skill_list handler 测试
//!
//! 验证跨 Group 技能聚合、排序和去重逻辑
//!
//! 测试用例：
//! 1. aggregates_skills_from_multiple_groups - 聚合多个 group 的技能
//! 2. sorts_by_confidence_then_occurrences - 按 confidence 降序、occurrences 降序排列
//! 3. deduplicates_skills_across_groups - 同名技能去重，保留最高 confidence
//! 4. empty_groups_returns_empty_list - 无 group 或无技能时返回空列表
//! 5. filters_by_min_confidence - min_confidence 参数过滤
//! 6. limits_result_count - limit 参数限制返回数量
//!
//! @author Atlas.oi
//! @date 2026-03-04

use std::collections::HashMap;

use ghostcode_daemon::server::AppState;
use ghostcode_daemon::skill_learning::{ingest_session_fragment, SessionFragment, SkillStore};
use ghostcode_types::ipc::DaemonRequest;

// ============================================
// 辅助函数：构建测试用 AppState 并插入候选技能
// ============================================

/// 构建包含指定 group 技能候选的 AppState
///
/// @param group_skills - map: group_id -> Vec<(problem, solution, confidence, occurrences)>
fn build_state_with_skills(
    group_skills: HashMap<&str, Vec<(&str, &str, u8, u32)>>,
) -> AppState {
    let state = AppState::default();
    {
        let mut store_map = state.skill_store.lock().unwrap();
        for (group_id, skills) in group_skills {
            let mut group_store = SkillStore::new();
            for (problem, solution, confidence, occurrences) in skills {
                // 第一次 ingest（创建候选）
                let fragment = SessionFragment {
                    problem: problem.to_string(),
                    solution: solution.to_string(),
                    confidence,
                    context: String::new(),
                    suggested_triggers: vec![],
                    suggested_tags: vec![],
                };
                ingest_session_fragment(&mut group_store, fragment);
                // 额外 ingest occurrences-1 次来模拟多次观察
                for _ in 1..occurrences {
                    let fragment = SessionFragment {
                        problem: problem.to_string(),
                        solution: solution.to_string(),
                        confidence,
                        context: String::new(),
                        suggested_triggers: vec![],
                        suggested_tags: vec![],
                    };
                    ingest_session_fragment(&mut group_store, fragment);
                }
            }
            store_map.insert(group_id.to_string(), group_store);
        }
    }
    state
}

/// 构建 team_skill_list 请求
fn make_request(args: serde_json::Value) -> DaemonRequest {
    DaemonRequest::new("team_skill_list", args)
}

// ============================================
// 测试用例
// ============================================

/// 测试 1: 聚合多个 group 的技能候选
///
/// 构造两个 group，各有不同候选技能，验证聚合后两者都出现在结果中
#[tokio::test]
async fn aggregates_skills_from_multiple_groups() {
    let mut group_skills = HashMap::new();
    group_skills.insert(
        "group-alpha",
        vec![
            ("修复 borrow 错误", "使用 clone 解决所有权问题", 80u8, 1u32),
        ],
    );
    group_skills.insert(
        "group-beta",
        vec![
            ("处理 async 生命周期", "使用 Arc 包装共享状态", 75u8, 1u32),
        ],
    );

    let state = build_state_with_skills(group_skills);
    let req = make_request(serde_json::json!({}));
    let resp = ghostcode_daemon::dispatch::dispatch(&state, req).await;

    assert!(resp.ok, "team_skill_list 应返回 ok");
    let skills = resp.result.as_array().expect("result 应为数组");
    // 两个 group 各一个候选，聚合后应有 2 条
    assert_eq!(skills.len(), 2, "应聚合 2 个 group 的技能");
}

/// 测试 2: 按 confidence 降序、occurrences 降序排序
///
/// 构造多个技能，验证排序顺序正确
#[tokio::test]
async fn sorts_by_confidence_then_occurrences() {
    let mut group_skills = HashMap::new();
    group_skills.insert(
        "group-alpha",
        vec![
            // confidence=80, occurrences=1
            ("问题A", "解决方案A", 80u8, 1u32),
            // confidence=90, occurrences=2
            ("问题B", "解决方案B", 90u8, 2u32),
            // confidence=80, occurrences=3（confidence 与 A 相同，但 occurrences 更高）
            ("问题C", "解决方案C", 80u8, 3u32),
        ],
    );

    let state = build_state_with_skills(group_skills);
    let req = make_request(serde_json::json!({}));
    let resp = ghostcode_daemon::dispatch::dispatch(&state, req).await;

    assert!(resp.ok);
    let skills = resp.result.as_array().expect("result 应为数组");
    assert_eq!(skills.len(), 3, "应有 3 个技能");

    // 第一名：confidence=90
    let first_confidence = skills[0]["confidence"].as_f64().unwrap();
    assert_eq!(first_confidence, 90.0, "第一名应是 confidence=90");

    // 第二名：confidence=80, occurrences=3
    let second_occurrences = skills[1]["total_occurrences"].as_u64().unwrap();
    assert_eq!(second_occurrences, 3, "第二名应是 occurrences=3（confidence 相同时 occurrences 降序）");

    // 第三名：confidence=80, occurrences=1
    let third_occurrences = skills[2]["total_occurrences"].as_u64().unwrap();
    assert_eq!(third_occurrences, 1, "第三名应是 occurrences=1");
}

/// 测试 3: 同名技能（相同 problem+solution）跨 group 去重，保留最高 confidence
///
/// 两个 group 都有相同 problem/solution 的候选，聚合后应只出现一次，
/// 且 confidence 保留较高者，source_groups 包含两个 group
#[tokio::test]
async fn deduplicates_skills_across_groups() {
    let mut group_skills = HashMap::new();
    // 两个 group 有相同的 problem+solution（同名技能）
    group_skills.insert(
        "group-alpha",
        vec![
            ("相同问题", "相同解决方案", 75u8, 1u32),
        ],
    );
    group_skills.insert(
        "group-beta",
        vec![
            ("相同问题", "相同解决方案", 85u8, 2u32),  // 更高 confidence
        ],
    );

    let state = build_state_with_skills(group_skills);
    let req = make_request(serde_json::json!({}));
    let resp = ghostcode_daemon::dispatch::dispatch(&state, req).await;

    assert!(resp.ok);
    let skills = resp.result.as_array().expect("result 应为数组");

    // 去重后只有 1 条
    assert_eq!(skills.len(), 1, "相同 problem+solution 应去重，只保留 1 条");

    // 保留最高 confidence
    let confidence = skills[0]["confidence"].as_f64().unwrap();
    assert_eq!(confidence, 85.0, "应保留最高 confidence 版本");

    // source_groups 应包含两个 group
    let source_groups = skills[0]["source_groups"].as_array().expect("source_groups 应为数组");
    assert_eq!(source_groups.len(), 2, "source_groups 应包含两个 group");
}

/// 测试 4: 无 group 或 group 无技能时返回空列表
///
/// 空的 skill_store 应返回空数组，而不是错误
#[tokio::test]
async fn empty_groups_returns_empty_list() {
    let state = AppState::default();
    let req = make_request(serde_json::json!({}));
    let resp = ghostcode_daemon::dispatch::dispatch(&state, req).await;

    assert!(resp.ok, "空 group 应返回 ok");
    let skills = resp.result.as_array().expect("result 应为数组");
    assert!(skills.is_empty(), "空 group 应返回空数组");
}

/// 测试 5: min_confidence 过滤
///
/// 只返回 confidence >= min_confidence 的技能
#[tokio::test]
async fn filters_by_min_confidence() {
    let mut group_skills = HashMap::new();
    group_skills.insert(
        "group-alpha",
        vec![
            ("低信号问题", "低信号方案", 72u8, 1u32),   // 低于过滤门槛
            ("高信号问题", "高信号方案", 90u8, 1u32),   // 高于过滤门槛
        ],
    );

    let state = build_state_with_skills(group_skills);
    // min_confidence=85.0，只应返回 confidence=90 的那个
    let req = make_request(serde_json::json!({ "min_confidence": 85.0 }));
    let resp = ghostcode_daemon::dispatch::dispatch(&state, req).await;

    assert!(resp.ok);
    let skills = resp.result.as_array().expect("result 应为数组");
    assert_eq!(skills.len(), 1, "应过滤掉低 confidence 技能");
    let confidence = skills[0]["confidence"].as_f64().unwrap();
    assert_eq!(confidence, 90.0, "返回的技能 confidence 应为 90");
}

/// 测试 6: limit 限制返回数量
///
/// 只返回 limit 条技能（按排序取前 N 条）
#[tokio::test]
async fn limits_result_count() {
    let mut group_skills = HashMap::new();
    group_skills.insert(
        "group-alpha",
        vec![
            ("问题1", "方案1", 80u8, 1u32),
            ("问题2", "方案2", 85u8, 1u32),
            ("问题3", "方案3", 90u8, 1u32),
        ],
    );

    let state = build_state_with_skills(group_skills);
    // limit=2，只返回前 2 条（confidence 最高的）
    let req = make_request(serde_json::json!({ "limit": 2 }));
    let resp = ghostcode_daemon::dispatch::dispatch(&state, req).await;

    assert!(resp.ok);
    let skills = resp.result.as_array().expect("result 应为数组");
    assert_eq!(skills.len(), 2, "limit=2 应只返回 2 条技能");
    // 应是 confidence 最高的前两条
    let first_confidence = skills[0]["confidence"].as_f64().unwrap();
    assert_eq!(first_confidence, 90.0, "第一条应是 confidence=90");
}
