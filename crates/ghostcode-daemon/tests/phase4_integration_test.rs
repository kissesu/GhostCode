//! Phase 4 端到端集成测试
//!
//! 验证 Dashboard、Skill Learning 模块的集成链路：
//! - 场景 1: Dashboard 快照查询（通过 dispatch 路由到 handler）
//! - 场景 2: Skill 提取流水线（ingest -> list -> promote）
//! - 场景 3: Dashboard + Skill 联合测试（skill 事件出现在 timeline 中）
//!
//! @author Atlas.oi
//! @date 2026-03-03

use ghostcode_daemon::dispatch::dispatch;
use ghostcode_daemon::server::AppState;
use ghostcode_types::event::{Event, EventKind};
use ghostcode_types::ipc::DaemonRequest;
use ghostcode_ledger::append_event;
use serde_json::json;
use tempfile::TempDir;

// ============================================
// 测试辅助函数
// ============================================

/// 构造 DaemonRequest 辅助函数
fn make_req(op: &str, args: serde_json::Value) -> DaemonRequest {
    DaemonRequest {
        v: 1,
        op: op.to_string(),
        args,
    }
}

/// 创建临时账本目录，并写入若干测试事件，返回配置了账本路径的 AppState
///
/// 账本路径约定: {groups_dir}/{group_id}/ledger.ndjson
/// 与 dashboard.rs 中的 ledger_path() 函数保持一致
async fn setup_ledger_with_events(event_count: usize) -> (TempDir, String, AppState) {
    let dir = TempDir::new().expect("创建临时目录失败");
    let group_id = "test-group-001";

    // groups_dir 就是 dir.path()，账本在 {dir}/{group_id}/ledger.ndjson
    let ledger_path = dir.path().join(group_id).join("ledger.ndjson");
    let lock_path = dir.path().join(group_id).join("ledger.lock");

    // 创建 group_id 子目录
    std::fs::create_dir_all(dir.path().join(group_id)).expect("创建 group 子目录失败");

    // 写入测试事件，使用多种 EventKind 和 actor
    let kinds = [
        EventKind::ChatMessage,
        EventKind::ActorStart,
        EventKind::SystemNotify,
        EventKind::ChatMessage,
        EventKind::ActorStop,
    ];

    for i in 0..event_count {
        let kind = kinds[i % kinds.len()].clone();
        let actor = format!("actor-{}", i % 3);
        let event = Event::new(
            kind,
            group_id,
            &format!("session-{}", i),
            &actor,
            json!({ "index": i, "text": format!("测试消息 {}", i) }),
        );
        append_event(&ledger_path, &lock_path, &event).expect("追加事件失败");
    }

    // AppState::new(groups_dir) -> 账本路径: groups_dir/{group_id}/ledger.ndjson
    let state = AppState::new(dir.path().to_path_buf());

    (dir, group_id.to_string(), state)
}

// ============================================
// 场景 1：Dashboard 快照查询
// ============================================

/// 验证 dashboard_snapshot 通过 dispatch 路由正常工作，返回事件数据
#[tokio::test]
async fn phase4_dashboard_snapshot_with_events() {
    let (_dir, group_id, state) = setup_ledger_with_events(5).await;

    let req = make_req("dashboard_snapshot", json!({ "group_id": group_id }));
    let resp = dispatch(&state, req).await;

    assert!(
        resp.ok,
        "dashboard_snapshot 应返回 ok=true，错误: {:?}",
        resp.error
    );

    // 验证 result 中包含 DashboardSnapshot 结构字段
    let data = &resp.result;
    assert!(
        data.get("total_events").is_some() || data.get("group_id").is_some(),
        "快照应包含 total_events 或 group_id 字段，实际: {}",
        data
    );
}

/// 验证 dashboard_timeline 通过 dispatch 路由正常工作，返回分页事件列表
#[tokio::test]
async fn phase4_dashboard_timeline_with_events() {
    let (_dir, group_id, state) = setup_ledger_with_events(5).await;

    let req = make_req(
        "dashboard_timeline",
        json!({ "group_id": group_id, "page_size": 10 }),
    );
    let resp = dispatch(&state, req).await;

    assert!(
        resp.ok,
        "dashboard_timeline 应返回 ok=true，错误: {:?}",
        resp.error
    );

    // dashboard_timeline 返回 TimelinePage { items, next_cursor, total }
    let data = &resp.result;
    assert!(
        data.get("items").is_some() || data.get("total").is_some(),
        "timeline 结果应包含 items 或 total 字段，实际: {}",
        data
    );

    // 验证写入了 5 个事件，timeline 应有事件
    let total = data["total"].as_u64().unwrap_or(0);
    assert!(total > 0, "timeline 应有事件，total={}", total);
}

/// 验证 dashboard_agents 通过 dispatch 路由正常工作，返回 agent 状态列表
#[tokio::test]
async fn phase4_dashboard_agents_with_events() {
    let (_dir, group_id, state) = setup_ledger_with_events(5).await;

    let req = make_req("dashboard_agents", json!({ "group_id": group_id }));
    let resp = dispatch(&state, req).await;

    assert!(
        resp.ok,
        "dashboard_agents 应返回 ok=true，错误: {:?}",
        resp.error
    );

    // 有事件时，应有 agents 列表
    let data = &resp.result;
    assert!(
        data.get("agents").is_some(),
        "dashboard_agents 应有 agents 字段，实际: {}",
        data
    );
}

// ============================================
// 场景 2：Skill 提取流水线
// ============================================

/// 验证 skill_learn_fragment -> skill_list -> skill_promote 完整流水线
#[tokio::test]
async fn phase4_skill_pipeline_learn_list_promote() {
    let state = AppState::default();
    // C4 修复：所有 skill 操作都需要传入 group_id 以实现 group 级别数据隔离
    let group_id = "test-group-alpha";

    // ============================================
    // 第一步：提交高置信度的会话片段（置信度 >= 70 才创建候选）
    // ============================================
    let learn_req = make_req(
        "skill_learn_fragment",
        json!({
            "group_id": group_id,
            "problem": "Rust 借用检查器报错：生命周期不匹配",
            "solution": "为结构体添加显式生命周期标注 'a，并在 impl 块中传播生命周期",
            "confidence": 85,
            "context": "在重构 HTTP 请求处理代码时遇到的生命周期问题",
            "suggested_triggers": ["rust", "lifetime", "borrow-checker"],
            "suggested_tags": ["rust", "fix", "lifetime"]
        }),
    );
    let learn_resp = dispatch(&state, learn_req).await;
    assert!(
        learn_resp.ok,
        "skill_learn_fragment 应返回 ok=true，错误: {:?}",
        learn_resp.error
    );

    // result 不应为 null（高置信度应返回创建的候选）
    assert!(
        !learn_resp.result.is_null(),
        "高置信度片段 learn_fragment 应返回候选数据"
    );

    // ============================================
    // 第二步：查询候选列表，确认候选已创建
    // ============================================
    let list_req = make_req("skill_list", json!({ "group_id": group_id }));
    let list_resp = dispatch(&state, list_req).await;
    assert!(
        list_resp.ok,
        "skill_list 应返回 ok=true，错误: {:?}",
        list_resp.error
    );

    let candidates = list_resp
        .result
        .as_array()
        .expect("skill_list 结果应为数组");
    assert!(!candidates.is_empty(), "高置信度片段应创建候选");

    let candidate_id = candidates[0]["id"]
        .as_str()
        .expect("候选应有 id 字段")
        .to_string();

    // ============================================
    // 第三步：提升第一个候选为正式 Skill
    // ============================================
    let promote_req = make_req(
        "skill_promote",
        json!({
            "group_id": group_id,
            "candidate_id": candidate_id,
            "skill_id": "fix-rust-lifetime",
            "skill_name": "修复 Rust 生命周期问题"
        }),
    );
    let promote_resp = dispatch(&state, promote_req).await;
    assert!(
        promote_resp.ok,
        "skill_promote 应返回 ok=true，错误: {:?}",
        promote_resp.error
    );

    // 验证提升结果包含 Skill 路径或元数据
    let skill = &promote_resp.result;
    assert!(
        skill.get("metadata").is_some() || skill.get("path").is_some(),
        "提升结果应包含 metadata 或 path 字段，实际: {}",
        skill
    );

    // ============================================
    // 第四步：再次查询列表，确认候选已从列表移除
    // ============================================
    let list_resp2 = dispatch(&state, make_req("skill_list", json!({ "group_id": group_id }))).await;
    assert!(list_resp2.ok, "第二次 skill_list 应返回 ok=true");

    let remaining_count = list_resp2
        .result
        .as_array()
        .map(|arr| arr.iter().filter(|c| c["id"].as_str() == Some(&candidate_id)).count())
        .unwrap_or(0);
    assert_eq!(remaining_count, 0, "已 promote 的候选应从 skill_list 中移除");
}

/// 验证低置信度片段不创建候选（质量门机制）
#[tokio::test]
async fn phase4_skill_low_confidence_rejected() {
    let state = AppState::default();
    // C4 修复：所有 skill 操作都需要传入 group_id 以实现 group 级别数据隔离
    let group_id = "test-group-low-confidence";

    // 提交低置信度片段（< 70），质量门应拦截
    let req = make_req(
        "skill_learn_fragment",
        json!({
            "group_id": group_id,
            "problem": "简单问题",
            "solution": "简单解答",
            "confidence": 40
        }),
    );
    let resp = dispatch(&state, req).await;
    assert!(resp.ok, "skill_learn_fragment 应返回 ok=true（低置信度也不报错）");

    // result 应为 null（低置信度不创建候选）
    assert!(
        resp.result.is_null(),
        "低置信度片段应返回 null result，实际: {}",
        resp.result
    );

    // 确认候选列表为空
    let list_resp = dispatch(&state, make_req("skill_list", json!({ "group_id": group_id }))).await;
    assert!(list_resp.ok, "skill_list 应返回 ok=true");
    let candidates = list_resp.result.as_array().cloned().unwrap_or_default();
    assert!(candidates.is_empty(), "低置信度片段不应创建候选");
}

// ============================================
// 场景 4：skill_extract 最小可行实现测试
// ============================================

/// 验证 skill_extract 不再返回 NOT_IMPLEMENTED 错误
///
/// Phase 6 Task 5：skill_extract 从 stub 升级为可用实现
#[tokio::test]
async fn skill_extract_no_longer_returns_not_implemented() {
    let state = AppState::default();
    let group_id = "test-group-extract";

    // skill_extract 传入基本参数，不应返回 NOT_IMPLEMENTED
    let req = make_req(
        "skill_extract",
        json!({
            "group_id": group_id,
            "problem": "如何处理 Rust 异步运行时错误",
            "solution": "使用 tokio::main 宏并在 async 函数中处理错误"
        }),
    );
    let resp = dispatch(&state, req).await;

    // 核心断言：不再返回 NOT_IMPLEMENTED
    if let Some(err) = &resp.error {
        assert_ne!(
            err.code, "NOT_IMPLEMENTED",
            "skill_extract 不应返回 NOT_IMPLEMENTED，实际错误: {:?}",
            err
        );
    }
}

/// 验证 skill_extract 从 transcript 提取片段创建候选
///
/// 输入高质量的 problem/solution 对，应该创建候选并返回候选数据
#[tokio::test]
async fn skill_extract_from_transcript_creates_candidate() {
    let state = AppState::default();
    let group_id = "test-group-extract-candidate";

    // 提交高置信度的 problem/solution 对
    let req = make_req(
        "skill_extract",
        json!({
            "group_id": group_id,
            "problem": "Rust 编译器报错：cannot borrow as mutable because it is also borrowed as immutable",
            "solution": "使用 RefCell 或在不同作用域中使用借用，避免同时存在可变和不可变引用"
        }),
    );
    let resp = dispatch(&state, req).await;

    // skill_extract 返回 ok
    assert!(
        resp.ok,
        "skill_extract 应返回 ok=true，错误: {:?}",
        resp.error
    );

    // 返回候选数据（非 null）
    assert!(
        !resp.result.is_null(),
        "skill_extract 应返回候选数据而非 null，实际: {}",
        resp.result
    );

    // 候选应有 id 字段
    assert!(
        resp.result.get("id").is_some(),
        "skill_extract 返回的候选应有 id 字段，实际: {}",
        resp.result
    );
}

/// 验证 skill_extract 低质量输入时返回 null（而非错误）
///
/// 当 problem 或 solution 为空时，启发式提取判断为低信号，返回 null candidate
#[tokio::test]
async fn skill_extract_low_signal_returns_null_candidate() {
    let state = AppState::default();
    let group_id = "test-group-extract-low-signal";

    // 提交空 problem，应为低信号
    let req = make_req(
        "skill_extract",
        json!({
            "group_id": group_id,
            "problem": "",
            "solution": ""
        }),
    );
    let resp = dispatch(&state, req).await;

    // 应返回 ok（不抛出错误）
    assert!(
        resp.ok,
        "skill_extract 空输入应返回 ok=true，错误: {:?}",
        resp.error
    );

    // 低信号应返回 null candidate
    assert!(
        resp.result.is_null(),
        "skill_extract 低信号应返回 null result，实际: {}",
        resp.result
    );
}

/// 验证 skill_extract 缺少 group_id 返回 INVALID_ARGS 错误
#[tokio::test]
async fn skill_extract_missing_group_id_returns_error() {
    let state = AppState::default();

    let req = make_req(
        "skill_extract",
        json!({
            "problem": "某个问题",
            "solution": "某个解决方案"
        }),
    );
    let resp = dispatch(&state, req).await;

    // 缺少 group_id 应返回错误
    assert!(!resp.ok, "缺少 group_id 的 skill_extract 应返回 ok=false");
    assert_eq!(
        resp.error.as_ref().unwrap().code,
        "INVALID_ARGS",
        "缺少 group_id 应返回 INVALID_ARGS 错误"
    );
}

/// 验证 skill_extract 不存在候选返回错误
#[tokio::test]
async fn phase4_skill_promote_nonexistent_returns_error() {
    let state = AppState::default();
    // C4 修复：传入 group_id 以满足 group 级别隔离要求
    let group_id = "test-group-promote-err";

    let req = make_req(
        "skill_promote",
        json!({
            "group_id": group_id,
            "candidate_id": "nonexistent-candidate-id",
            "skill_id": "test-skill"
        }),
    );
    let resp = dispatch(&state, req).await;
    assert!(!resp.ok, "promote 不存在的候选应返回 ok=false");
    assert!(resp.error.is_some(), "应有错误信息");
}

// ============================================
// 场景 3：Dashboard + Skill 联合测试
// ============================================

/// 验证 skill 相关事件写入账本后，能通过 dashboard 查询到
#[tokio::test]
async fn phase4_dashboard_reflects_skill_events() {
    let dir = TempDir::new().expect("创建临时目录失败");
    let group_id = "skill-group-001";

    // 账本路径: groups_dir/{group_id}/ledger.ndjson
    let ledger_path = dir.path().join(group_id).join("ledger.ndjson");
    let lock_path = dir.path().join(group_id).join("ledger.lock");
    std::fs::create_dir_all(dir.path().join(group_id)).expect("创建 group 子目录失败");

    // 写入 SkillLearned 事件到账本
    let skill_event = Event::new(
        EventKind::SkillLearned,
        group_id,
        "session-skill",
        "agent-builder",
        json!({
            "skill_id": "fix-rust-lifetime",
            "skill_name": "修复 Rust 生命周期",
            "confidence": 85
        }),
    );
    append_event(&ledger_path, &lock_path, &skill_event).expect("写入 SkillLearned 事件失败");

    // 写入 SkillPromoted 事件到账本
    let promote_event = Event::new(
        EventKind::SkillPromoted,
        group_id,
        "session-skill",
        "agent-builder",
        json!({
            "skill_id": "fix-rust-lifetime",
            "skill_name": "修复 Rust 生命周期"
        }),
    );
    append_event(&ledger_path, &lock_path, &promote_event).expect("写入 SkillPromoted 事件失败");

    // AppState 使用 dir.path() 作为 groups_dir
    let state = AppState::new(dir.path().to_path_buf());

    // 通过 dashboard_timeline 查询，验证 skill 事件出现在 timeline 中
    let req = make_req(
        "dashboard_timeline",
        json!({ "group_id": group_id, "page_size": 20 }),
    );
    let resp = dispatch(&state, req).await;
    assert!(
        resp.ok,
        "dashboard_timeline 应返回 ok=true，错误: {:?}",
        resp.error
    );

    // 验证 timeline 包含写入的 2 个 skill 事件
    let total = resp.result["total"].as_u64().unwrap_or(0);
    assert!(
        total >= 2,
        "timeline 应包含至少 2 个 skill 相关事件，total={}",
        total
    );

    // 验证 dashboard_snapshot 也能正常查询
    let snapshot_req = make_req("dashboard_snapshot", json!({ "group_id": group_id }));
    let snapshot_resp = dispatch(&state, snapshot_req).await;
    assert!(
        snapshot_resp.ok,
        "dashboard_snapshot 应返回 ok=true，错误: {:?}",
        snapshot_resp.error
    );

    // total_events 应等于写入的事件数
    let total_events = snapshot_resp.result["total_events"].as_u64().unwrap_or(0);
    assert_eq!(total_events, 2, "snapshot total_events 应为 2");
}
