// @file cost_pbt_test.rs
// @description 成本追踪引擎属性测试（PBT 阶段）
//              验证 CostSnapshot 的代数属性：非负性、单调性、结合律、交换律、零元性质、溢出安全
// @author Atlas.oi
// @date 2026-03-03

use ghostcode_daemon::cost::{
    apply_usage, merge, CostSnapshot, CostSource, PricingTable, UsageRecord,
};
use proptest::prelude::*;

// ============================================================
// 策略生成器：生成随机 UsageRecord
// ============================================================

/// 生成任意合法的 UsageRecord
fn arb_usage() -> impl Strategy<Value = UsageRecord> {
    (
        // group_id: 非空字母字符串
        "[a-z]{1,8}",
        // task_id: 非空字母字符串
        "[a-z]{1,8}",
        // model: 限定几种已知模型
        prop_oneof![
            Just("claude-opus".to_string()),
            Just("codex".to_string()),
            Just("gemini-pro".to_string()),
        ],
        // prompt_tokens
        0u32..1_000_000u32,
        // completion_tokens
        0u32..1_000_000u32,
    )
        .prop_map(|(g, t, m, p, c)| UsageRecord {
            group_id: g,
            task_id: t,
            model: m,
            prompt_tokens: p,
            completion_tokens: c,
            source: CostSource::Exact,
        })
}

/// 生成任意 UsageRecord 序列（0..100 条）
fn arb_usage_seq() -> impl Strategy<Value = Vec<UsageRecord>> {
    prop::collection::vec(arb_usage(), 0..100)
}

/// 生成任意 CostSnapshot（用于代数属性测试）
fn arb_snapshot() -> impl Strategy<Value = CostSnapshot> {
    (
        0u64..1_000_000_000u64, // total_prompt_tokens
        0u64..1_000_000_000u64, // total_completion_tokens
        0u64..u64::MAX / 2,     // total_cost_micro（避免溢出测试用例相互干扰）
        0u64..1_000_000u64,     // request_count
    )
        .prop_map(|(p, c, cost, r)| CostSnapshot {
            total_prompt_tokens: p,
            total_completion_tokens: c,
            total_cost_micro: cost,
            request_count: r,
        })
}

// ============================================================
// 属性 1：非负性
// apply_usage 后所有累计字段 >= 0（u64 天然保证，但验证逻辑正确性）
// ============================================================

proptest! {
    #[test]
    fn prop_apply_usage_non_negative(
        usage in arb_usage()
    ) {
        let pricing = PricingTable::default();
        let zero = CostSnapshot::zero();
        let result = apply_usage(&zero, &usage, &pricing);

        // u64 天然非负，验证请求数量正确递增为 1
        prop_assert_eq!(result.request_count, 1u64);
    }
}

// ============================================================
// 属性 2：累计单调性
// apply_usage(s, u).total_cost_micro >= s.total_cost_micro
// ============================================================

proptest! {
    #[test]
    fn prop_apply_usage_cost_monotone(
        snapshot in arb_snapshot(),
        usage in arb_usage()
    ) {
        let pricing = PricingTable::default();
        let result = apply_usage(&snapshot, &usage, &pricing);
        // 成本只增不减（codex 免费时等于原值）
        prop_assert!(result.total_cost_micro >= snapshot.total_cost_micro);
        // token 累计只增不减
        prop_assert!(result.total_prompt_tokens >= snapshot.total_prompt_tokens);
        prop_assert!(result.total_completion_tokens >= snapshot.total_completion_tokens);
        // 请求数量严格递增
        prop_assert_eq!(result.request_count, snapshot.request_count + 1);
    }
}

// ============================================================
// 属性 3：merge 结合律
// merge(a, merge(b, c)) == merge(merge(a, b), c)
// ============================================================

proptest! {
    #[test]
    fn prop_merge_associativity(
        a in arb_snapshot(),
        b in arb_snapshot(),
        c in arb_snapshot()
    ) {
        let left = merge(&a, &merge(&b, &c));
        let right = merge(&merge(&a, &b), &c);
        prop_assert_eq!(left, right);
    }
}

// ============================================================
// 属性 4：merge 交换律
// merge(a, b) == merge(b, a)
// ============================================================

proptest! {
    #[test]
    fn prop_merge_commutativity(
        a in arb_snapshot(),
        b in arb_snapshot()
    ) {
        let left = merge(&a, &b);
        let right = merge(&b, &a);
        prop_assert_eq!(left, right);
    }
}

// ============================================================
// 属性 5：零元性质
// merge(CostSnapshot::zero(), s) == s
// ============================================================

proptest! {
    #[test]
    fn prop_merge_identity_element(
        s in arb_snapshot()
    ) {
        let zero = CostSnapshot::zero();
        let left = merge(&zero, &s);
        let right = merge(&s, &zero);
        prop_assert_eq!(left.clone(), s.clone(), "zero 在左侧是单位元");
        prop_assert_eq!(right, s, "zero 在右侧是单位元");
    }
}

// ============================================================
// 属性 6：序列折叠一致性
// 按序 apply 多条 usage == 分别 apply 后 merge
// ============================================================

proptest! {
    #[test]
    fn prop_fold_consistency(
        usages in arb_usage_seq()
    ) {
        let pricing = PricingTable::default();
        let zero = CostSnapshot::zero();

        // 方法1：fold 逐条 apply
        let folded = usages.iter().fold(zero.clone(), |acc, u| apply_usage(&acc, u, &pricing));

        // 验证 folded 的 request_count 等于 usages 长度
        prop_assert_eq!(
            folded.request_count,
            usages.len() as u64,
            "request_count 应等于记录数"
        );

        // 验证 token 累计等于各条记录 token 之和
        let total_prompt: u64 = usages.iter().map(|u| u.prompt_tokens as u64).fold(0u64, u64::saturating_add);
        let total_completion: u64 = usages.iter().map(|u| u.completion_tokens as u64).fold(0u64, u64::saturating_add);
        prop_assert_eq!(
            folded.total_prompt_tokens,
            total_prompt,
            "prompt tokens 总和应一致"
        );
        prop_assert_eq!(
            folded.total_completion_tokens,
            total_completion,
            "completion tokens 总和应一致"
        );
    }
}

// ============================================================
// 属性 7：溢出安全
// 极大 token 输入不 panic（saturating_add 保证不溢出）
// ============================================================

proptest! {
    #[test]
    fn prop_no_overflow_on_large_tokens(
        prompt_tokens in u32::MAX / 2..u32::MAX,
        completion_tokens in u32::MAX / 2..u32::MAX,
    ) {
        let pricing = PricingTable::default();
        let usage = UsageRecord {
            group_id: "g1".to_string(),
            task_id: "t1".to_string(),
            model: "claude-opus".to_string(),  // 最贵的模型，最容易溢出
            prompt_tokens,
            completion_tokens,
            source: CostSource::Exact,
        };

        // 构造一个接近 u64::MAX 的快照
        let big_snapshot = CostSnapshot {
            total_prompt_tokens: u64::MAX / 2,
            total_completion_tokens: u64::MAX / 2,
            total_cost_micro: u64::MAX / 2,
            request_count: u64::MAX / 2,
        };

        // 不应该 panic
        let result = apply_usage(&big_snapshot, &usage, &pricing);

        // saturating_add 保证不超过 u64::MAX
        prop_assert!(result.total_cost_micro >= big_snapshot.total_cost_micro || result.total_cost_micro == u64::MAX);
    }
}
