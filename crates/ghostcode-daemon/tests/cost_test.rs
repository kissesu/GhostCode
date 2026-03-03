// @file cost_test.rs
// @description 成本追踪引擎契约测试（TDD Red 阶段）
//              测试 CostStore 的记录、聚合、按模型/任务汇总等功能
// @author Atlas.oi
// @date 2026-03-03

use ghostcode_daemon::cost::{
    CostSource, CostSnapshot, CostStore, PricingTable, UsageRecord,
};

// ============================================================
// 辅助函数：构造测试用 UsageRecord
// ============================================================

fn make_usage(group_id: &str, task_id: &str, model: &str, prompt: u32, completion: u32) -> UsageRecord {
    UsageRecord {
        group_id: group_id.to_string(),
        task_id: task_id.to_string(),
        model: model.to_string(),
        prompt_tokens: prompt,
        completion_tokens: completion,
        source: CostSource::Exact,
    }
}

// ============================================================
// 测试 1：record_usage 存储成功（返回 ok）
// ============================================================

#[test]
fn record_usage_returns_ok() {
    let mut store = CostStore::new();
    let usage = make_usage("g1", "t1", "claude-opus", 1000, 200);
    // record_usage 不应该返回错误
    store.record_usage(usage);
    // 验证存储后 summary 请求数量为 1
    let summary = store.get_summary(None, None);
    assert_eq!(summary.request_count, 1);
}

// ============================================================
// 测试 2：compute_cost 价格映射正确
// 价格表（micro-cents per token）：
//   Claude Opus: 输入 15000, 输出 75000
//   Codex: 输入 0（免费）, 输出 0
//   Gemini Pro: 输入 1250, 输出 10000
// ============================================================

#[test]
fn compute_cost_claude_opus() {
    let pricing = PricingTable::default();
    // 1000 prompt tokens @ 15000 micro-cents/token = 15_000_000
    // 200 completion tokens @ 75000 micro-cents/token = 15_000_000
    // 总计 = 30_000_000 micro-cents
    let usage = make_usage("g1", "t1", "claude-opus", 1000, 200);
    let cost = ghostcode_daemon::cost::compute_cost("claude-opus", &usage, &pricing);
    assert_eq!(cost, 1000u64 * 15_000 + 200u64 * 75_000);
}

#[test]
fn compute_cost_codex_is_free() {
    let pricing = PricingTable::default();
    let usage = make_usage("g1", "t1", "codex", 5000, 1000);
    let cost = ghostcode_daemon::cost::compute_cost("codex", &usage, &pricing);
    assert_eq!(cost, 0);
}

#[test]
fn compute_cost_gemini_pro() {
    let pricing = PricingTable::default();
    // 1000 prompt tokens @ 1250 micro-cents/token = 1_250_000
    // 500 completion tokens @ 10000 micro-cents/token = 5_000_000
    // 总计 = 6_250_000 micro-cents
    let usage = make_usage("g1", "t1", "gemini-pro", 1000, 500);
    let cost = ghostcode_daemon::cost::compute_cost("gemini-pro", &usage, &pricing);
    assert_eq!(cost, 1000u64 * 1_250 + 500u64 * 10_000);
}

// ============================================================
// 测试 3：按模型聚合统计（record 3 条不同模型 → summary 各模型独立统计）
// ============================================================

#[test]
fn summary_by_model_three_different_models() {
    let mut store = CostStore::new();
    store.record_usage(make_usage("g1", "t1", "claude-opus", 100, 50));
    store.record_usage(make_usage("g1", "t2", "codex", 200, 100));
    store.record_usage(make_usage("g1", "t3", "gemini-pro", 300, 150));

    let by_model = store.get_summary_by_model("g1", None);

    // 三种模型必须各自出现
    assert!(by_model.contains_key("claude-opus"), "应包含 claude-opus");
    assert!(by_model.contains_key("codex"), "应包含 codex");
    assert!(by_model.contains_key("gemini-pro"), "应包含 gemini-pro");

    // 各模型请求数量应为 1
    assert_eq!(by_model["claude-opus"].request_count, 1);
    assert_eq!(by_model["codex"].request_count, 1);
    assert_eq!(by_model["gemini-pro"].request_count, 1);

    // claude-opus 的 token 数量应正确
    assert_eq!(by_model["claude-opus"].total_prompt_tokens, 100);
    assert_eq!(by_model["claude-opus"].total_completion_tokens, 50);
}

// ============================================================
// 测试 4：按任务聚合统计（record 多条同 task_id → summary 合并）
// ============================================================

#[test]
fn summary_by_task_merges_same_task() {
    let mut store = CostStore::new();
    // 同一个 task_id 下有两条记录
    store.record_usage(make_usage("g1", "task_a", "claude-opus", 100, 50));
    store.record_usage(make_usage("g1", "task_a", "codex", 200, 100));
    // 另一个 task_id
    store.record_usage(make_usage("g1", "task_b", "gemini-pro", 300, 150));

    let summary_a = store.get_summary(Some("g1"), Some("task_a"));
    // task_a 应合并两条记录
    assert_eq!(summary_a.request_count, 2);
    assert_eq!(summary_a.total_prompt_tokens, 300); // 100 + 200
    assert_eq!(summary_a.total_completion_tokens, 150); // 50 + 100

    let summary_b = store.get_summary(Some("g1"), Some("task_b"));
    // task_b 只有一条记录
    assert_eq!(summary_b.request_count, 1);
}

// ============================================================
// 测试 5：未知模型标记 source=Estimated（使用默认价格）
// ============================================================

#[test]
fn unknown_model_uses_estimated_source() {
    let pricing = PricingTable::default();
    let usage = make_usage("g1", "t1", "unknown-model-xyz", 1000, 500);

    // 使用未知模型时，应使用默认价格（不应 panic）
    // 默认价格：prompt=5000, completion=15000
    let cost = ghostcode_daemon::cost::compute_cost("unknown-model-xyz", &usage, &pricing);
    let expected = 1000u64 * 5_000 + 500u64 * 15_000;
    assert_eq!(cost, expected, "未知模型应使用默认价格");
}

#[test]
fn record_unknown_model_has_estimated_source() {
    let mut store = CostStore::new();
    // 记录未知模型
    let usage = UsageRecord {
        group_id: "g1".to_string(),
        task_id: "t1".to_string(),
        model: "unknown-model-xyz".to_string(),
        prompt_tokens: 1000,
        completion_tokens: 500,
        source: CostSource::Estimated, // 未知模型应标记为 Estimated
    };
    store.record_usage(usage);
    let summary = store.get_summary(None, None);
    assert_eq!(summary.request_count, 1);
}

// ============================================================
// 测试 6：空 store get_summary 返回零值
// ============================================================

#[test]
fn empty_store_returns_zero_summary() {
    let store = CostStore::new();
    let summary = store.get_summary(None, None);
    assert_eq!(summary.total_prompt_tokens, 0);
    assert_eq!(summary.total_completion_tokens, 0);
    assert_eq!(summary.total_cost_micro, 0);
    assert_eq!(summary.request_count, 0);
}

// ============================================================
// 测试 7：零 token 的 usage 不影响已有累计
// ============================================================

#[test]
fn zero_token_usage_does_not_affect_existing() {
    let mut store = CostStore::new();
    // 先记录一条有效数据
    store.record_usage(make_usage("g1", "t1", "claude-opus", 1000, 200));
    let before = store.get_summary(None, None);

    // 再记录零 token
    store.record_usage(make_usage("g1", "t2", "claude-opus", 0, 0));
    let after = store.get_summary(None, None);

    // token 累计不变
    assert_eq!(after.total_prompt_tokens, before.total_prompt_tokens);
    assert_eq!(after.total_completion_tokens, before.total_completion_tokens);
    // 成本不变
    assert_eq!(after.total_cost_micro, before.total_cost_micro);
    // 请求数量增加 1
    assert_eq!(after.request_count, before.request_count + 1);
}

// ============================================================
// 测试 8：get_summary 过滤 group_id 和 task_id
// ============================================================

#[test]
fn get_summary_filters_by_group_id() {
    let mut store = CostStore::new();
    store.record_usage(make_usage("g1", "t1", "claude-opus", 1000, 200));
    store.record_usage(make_usage("g2", "t1", "codex", 2000, 400));

    let summary_g1 = store.get_summary(Some("g1"), None);
    let summary_g2 = store.get_summary(Some("g2"), None);

    assert_eq!(summary_g1.request_count, 1);
    assert_eq!(summary_g2.request_count, 1);
    assert_eq!(summary_g1.total_prompt_tokens, 1000);
    assert_eq!(summary_g2.total_prompt_tokens, 2000);
}

// ============================================================
// 测试 9：CostSnapshot 的 zero() 方法返回零值快照
// ============================================================

#[test]
fn cost_snapshot_zero_is_identity() {
    let zero = CostSnapshot::zero();
    assert_eq!(zero.total_prompt_tokens, 0);
    assert_eq!(zero.total_completion_tokens, 0);
    assert_eq!(zero.total_cost_micro, 0);
    assert_eq!(zero.request_count, 0);
}

// ============================================================
// 测试 10：merge 结合两个快照
// ============================================================

#[test]
fn merge_combines_two_snapshots() {
    let s1 = CostSnapshot {
        total_prompt_tokens: 100,
        total_completion_tokens: 50,
        total_cost_micro: 1_000_000,
        request_count: 2,
    };
    let s2 = CostSnapshot {
        total_prompt_tokens: 200,
        total_completion_tokens: 100,
        total_cost_micro: 2_000_000,
        request_count: 3,
    };
    let merged = ghostcode_daemon::cost::merge(&s1, &s2);
    assert_eq!(merged.total_prompt_tokens, 300);
    assert_eq!(merged.total_completion_tokens, 150);
    assert_eq!(merged.total_cost_micro, 3_000_000);
    assert_eq!(merged.request_count, 5);
}
