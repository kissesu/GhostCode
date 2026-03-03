// @file hud_snapshot_test.rs
// @description HUD 快照聚合契约测试
//              测试 build_hud_snapshot 的各种场景：
//              空状态、有验证、有成本、同时有验证和成本、上下文压力级别
// @author Atlas.oi
// @date 2026-03-03

use ghostcode_daemon::hud::{build_hud_snapshot, compute_context_level};
use ghostcode_daemon::server::AppState;
use ghostcode_daemon::cost::{CostSource, UsageRecord};

// ============================================
// 辅助函数
// ============================================

/// 创建测试用 AppState
fn make_state() -> AppState {
    AppState::default()
}

// ============================================
// 测试：空状态返回安全默认值
// ============================================

// 空状态下调用 build_hud_snapshot，所有字段应为零/空/默认值
#[test]
fn hud_snapshot_empty_state() {
    let state = make_state();
    let args = serde_json::json!({});
    let snapshot = build_hud_snapshot(&state, &args);

    // 验证摘要为空（未指定 group_id + run_id）
    assert!(snapshot.verification.is_none(), "空状态下 verification 应为 None");

    // 成本全为零
    assert_eq!(snapshot.cost.total_cost_micro, 0, "空状态下 total_cost_micro 应为 0");
    assert_eq!(snapshot.cost.total_prompt_tokens, 0, "空状态下 total_prompt_tokens 应为 0");
    assert_eq!(snapshot.cost.total_completion_tokens, 0, "空状态下 total_completion_tokens 应为 0");
    assert_eq!(snapshot.cost.request_count, 0, "空状态下 request_count 应为 0");

    // 上下文压力：未传入时默认为 0/0，百分比为 0，级别 green
    assert_eq!(snapshot.context_pressure.used_tokens, 0);
    assert_eq!(snapshot.context_pressure.max_tokens, 0);
    assert_eq!(snapshot.context_pressure.level, "green");

    // 活跃 Agent 数量为 Some(0)（空状态下 RwLock 可读，值为 0）
    assert_eq!(snapshot.active_agents, Some(0), "空状态下 active_agents 应为 Some(0)");
}

// ============================================
// 测试：有验证运行时返回验证状态摘要
// ============================================

// 创建验证运行后，传入 group_id + run_id 应返回验证摘要
#[test]
fn hud_snapshot_with_verification() {
    let state = make_state();

    // 创建验证运行
    {
        let mut store = state.verification.lock().unwrap();
        store.start_run("group1".to_string(), "run1".to_string())
            .expect("start_run 应成功");
    }

    let args = serde_json::json!({
        "group_id": "group1",
        "run_id": "run1",
    });
    let snapshot = build_hud_snapshot(&state, &args);

    // 验证摘要不为空
    let verification = snapshot.verification.expect("有验证运行时 verification 不应为 None");

    assert_eq!(verification.run_id, "run1");
    assert_eq!(verification.group_id, "group1");
    assert_eq!(verification.status, "Running");
    assert_eq!(verification.iteration, 0);
    assert_eq!(verification.max_iterations, 10);
    // 初始状态 7 项均为 Pending，checks_passed = 0，checks_total = 7
    assert_eq!(verification.checks_passed, 0);
    assert_eq!(verification.checks_total, 7);
}

// ============================================
// 测试：有成本记录时返回成本汇总
// ============================================

// 记录成本后，快照中的成本字段应反映累计数据
#[test]
fn hud_snapshot_with_cost() {
    let state = make_state();

    // 记录一条使用记录
    {
        let mut store = state.costs.lock().unwrap();
        store.record_usage(UsageRecord {
            group_id: "group1".to_string(),
            task_id: "task1".to_string(),
            model: "claude-sonnet".to_string(),
            prompt_tokens: 100,
            completion_tokens: 50,
            source: CostSource::VendorReported,
        });
    }

    let args = serde_json::json!({
        "group_id": "group1",
    });
    let snapshot = build_hud_snapshot(&state, &args);

    // 成本字段应有值
    assert_eq!(snapshot.cost.request_count, 1, "request_count 应为 1");
    assert_eq!(snapshot.cost.total_prompt_tokens, 100, "total_prompt_tokens 应为 100");
    assert_eq!(snapshot.cost.total_completion_tokens, 50, "total_completion_tokens 应为 50");
    // claude-sonnet: prompt=3_000, completion=15_000 micro-cents/token
    // cost = 100 * 3_000 + 50 * 15_000 = 300_000 + 750_000 = 1_050_000
    assert_eq!(snapshot.cost.total_cost_micro, 1_050_000);
}

// ============================================
// 测试：同时有验证和成本时正确聚合
// ============================================

// 同时存在验证运行和成本记录时，两个字段均应正确填充
#[test]
fn hud_snapshot_with_both() {
    let state = make_state();

    // 创建验证运行
    {
        let mut store = state.verification.lock().unwrap();
        store.start_run("group2".to_string(), "run2".to_string())
            .expect("start_run 应成功");
    }

    // 记录成本
    {
        let mut store = state.costs.lock().unwrap();
        store.record_usage(UsageRecord {
            group_id: "group2".to_string(),
            task_id: "task2".to_string(),
            model: "claude-opus".to_string(),
            prompt_tokens: 200,
            completion_tokens: 100,
            source: CostSource::Exact,
        });
    }

    let args = serde_json::json!({
        "group_id": "group2",
        "run_id": "run2",
    });
    let snapshot = build_hud_snapshot(&state, &args);

    // 验证摘要存在
    let verification = snapshot.verification.expect("verification 不应为 None");
    assert_eq!(verification.status, "Running");

    // 成本字段有值
    assert_eq!(snapshot.cost.request_count, 1);
    assert_eq!(snapshot.cost.total_prompt_tokens, 200);
}

// ============================================
// 测试：上下文压力级别判定
// ============================================

// used_tokens/max_tokens < 70% 时上下文压力为 green
#[test]
fn hud_snapshot_context_pressure_green() {
    let state = make_state();
    // 65% 上下文占用
    let args = serde_json::json!({
        "used_tokens": 6500u64,
        "max_tokens": 10000u64,
    });
    let snapshot = build_hud_snapshot(&state, &args);

    assert_eq!(snapshot.context_pressure.level, "green");
    assert_eq!(snapshot.context_pressure.used_tokens, 6500);
    assert_eq!(snapshot.context_pressure.max_tokens, 10000);
    assert!((snapshot.context_pressure.percentage - 65.0).abs() < 0.01);
}

// used_tokens/max_tokens 在 70%-85% 之间时上下文压力为 yellow
#[test]
fn hud_snapshot_context_pressure_yellow() {
    let state = make_state();
    // 75% 上下文占用
    let args = serde_json::json!({
        "used_tokens": 7500u64,
        "max_tokens": 10000u64,
    });
    let snapshot = build_hud_snapshot(&state, &args);

    assert_eq!(snapshot.context_pressure.level, "yellow");
    assert!((snapshot.context_pressure.percentage - 75.0).abs() < 0.01);
}

// used_tokens/max_tokens > 85% 时上下文压力为 red
#[test]
fn hud_snapshot_context_pressure_red() {
    let state = make_state();
    // 90% 上下文占用
    let args = serde_json::json!({
        "used_tokens": 9000u64,
        "max_tokens": 10000u64,
    });
    let snapshot = build_hud_snapshot(&state, &args);

    assert_eq!(snapshot.context_pressure.level, "red");
    assert!((snapshot.context_pressure.percentage - 90.0).abs() < 0.01);
}

// ============================================
// 测试：通过 dispatch 调用 hud_snapshot op
// ============================================

// 通过 dispatch 调用 hud_snapshot op 应返回 ok: true 且包含正确字段
#[tokio::test]
async fn hud_snapshot_via_dispatch() {
    use ghostcode_daemon::dispatch::dispatch;
    use ghostcode_types::ipc::DaemonRequest;

    let state = make_state();
    let req = DaemonRequest::new("hud_snapshot", serde_json::json!({}));

    let resp = dispatch(&state, req).await;

    // 响应应为 ok
    assert!(resp.ok, "hud_snapshot dispatch 响应应为 ok");

    // 响应数据应包含必需字段（在 result 字段中）
    let data = &resp.result;
    assert!(data.get("cost").is_some(), "响应中应有 cost 字段");
    assert!(data.get("context_pressure").is_some(), "响应中应有 context_pressure 字段");
    assert!(data.get("active_agents").is_some(), "响应中应有 active_agents 字段");
}

// ============================================
// 测试：compute_context_level 纯函数
// ============================================

// 验证 compute_context_level 的边界值行为
#[test]
fn compute_context_level_boundaries() {
    assert_eq!(compute_context_level(0.0), "green");
    assert_eq!(compute_context_level(69.9), "green");
    assert_eq!(compute_context_level(70.0), "yellow");
    assert_eq!(compute_context_level(85.0), "yellow");
    assert_eq!(compute_context_level(85.1), "red");
    assert_eq!(compute_context_level(100.0), "red");
}
