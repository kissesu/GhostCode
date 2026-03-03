//! Phase 3 端到端集成测试
//!
//! @file phase3_integration_test.rs
//! @description 覆盖 Phase 3（验证、成本、HUD）所有模块的端到端集成测试
//!              测试验证、成本记录、HUD 快照的完整业务流程链路：
//!              - verification 生命周期（启动 → 推进 → 批准/取消/失败重试）
//!              - cost 记录与多维度聚合
//!              - hud_snapshot 全链路聚合
//!              - 完整管线：verification → cost → hud 串联
//! @author Atlas.oi
//! @date 2026-03-03

use ghostcode_daemon::dispatch::dispatch;
use ghostcode_daemon::server::AppState;
use ghostcode_daemon::verification::{VerificationCheckKind, VerifyEvent};
use ghostcode_types::ipc::DaemonRequest;

// ============================================
// 辅助函数
// ============================================

/// 构造 verification_start 请求并断言成功
///
/// @param state - 应用状态
/// @param group_id - Group ID
/// @param run_id - Run ID
async fn start_verification(state: &AppState, group_id: &str, run_id: &str) {
    let req = DaemonRequest::new(
        "verification_start",
        serde_json::json!({ "group_id": group_id, "run_id": run_id }),
    );
    let resp = dispatch(state, req).await;
    assert!(
        resp.ok,
        "verification_start 应成功，实际错误：{:?}",
        resp.error
    );
}

/// 对 state.verification 应用 7 个 CheckPassed 事件（覆盖所有检查类型）
///
/// @param state - 应用状态
/// @param group_id - Group ID
/// @param run_id - Run ID
fn apply_all_checks_passed(state: &AppState, group_id: &str, run_id: &str) {
    let checks = vec![
        VerificationCheckKind::Build,
        VerificationCheckKind::Test,
        VerificationCheckKind::Lint,
        VerificationCheckKind::Functionality,
        VerificationCheckKind::Architect,
        VerificationCheckKind::Todo,
        VerificationCheckKind::ErrorFree,
    ];

    let mut store = state.verification.lock().unwrap();
    for check in checks {
        store
            .apply_event(group_id, run_id, VerifyEvent::CheckPassed(check))
            .expect("apply_event CheckPassed 应成功");
    }
}

// ============================================
// 测试 1：完整验证生命周期
// ============================================

/// e2e 测试：验证完整生命周期（start → apply 7 个 CheckPassed → AdvanceIteration → Approved）
///
/// 验证步骤：
/// 1. verification_start → 确认返回 Running 状态
/// 2. 通过 state.verification.lock() 直接 apply 7 个 CheckPassed 事件
/// 3. 再 apply AdvanceIteration 事件
/// 4. verification_status → 确认返回 Approved 状态
/// 5. 验证 iteration=0, checks 全部通过
#[tokio::test]
async fn e2e_verification_full_lifecycle() {
    let state = AppState::default();
    let group_id = "g-lifecycle";
    let run_id = "r-lifecycle";

    // 第一步：启动验证运行，期望 Running 状态
    let req_start = DaemonRequest::new(
        "verification_start",
        serde_json::json!({ "group_id": group_id, "run_id": run_id }),
    );
    let resp_start = dispatch(&state, req_start).await;
    assert!(resp_start.ok, "verification_start 应成功");
    assert_eq!(
        resp_start.result["status"].as_str(),
        Some("Running"),
        "启动后状态应为 Running"
    );
    assert_eq!(
        resp_start.result["iteration"].as_u64(),
        Some(0),
        "初始 iteration 应为 0"
    );

    // 第二步：通过 state.verification.lock() 直接 apply 7 个 CheckPassed 事件
    apply_all_checks_passed(&state, group_id, run_id);

    // 第三步：apply AdvanceIteration 事件，推进迭代结算
    {
        let mut store = state.verification.lock().unwrap();
        store
            .apply_event(group_id, run_id, VerifyEvent::AdvanceIteration)
            .expect("AdvanceIteration 应成功");
    }

    // 第四步：查询 verification_status，期望 Approved 状态
    let req_status = DaemonRequest::new(
        "verification_status",
        serde_json::json!({ "group_id": group_id, "run_id": run_id }),
    );
    let resp_status = dispatch(&state, req_status).await;
    assert!(resp_status.ok, "verification_status 应成功");
    assert_eq!(
        resp_status.result["status"].as_str(),
        Some("Approved"),
        "所有检查通过并 AdvanceIteration 后应为 Approved"
    );

    // 第五步：验证 iteration 仍为 0（一次通过，不需要增加迭代）
    assert_eq!(
        resp_status.result["iteration"].as_u64(),
        Some(0),
        "一次通过时 iteration 应仍为 0"
    );
}

// ============================================
// 测试 2：验证失败和重试
// ============================================

/// e2e 测试：验证失败后重试，最终批准
///
/// 验证步骤：
/// 1. verification_start → Running
/// 2. Apply CheckPassed(Build), CheckFailed(Test, "test 失败")
/// 3. Apply AdvanceIteration → 仍为 Running，iteration=1
/// 4. verification_status 确认 iteration=1
/// 5. Apply 7 个 CheckPassed + AdvanceIteration → Approved
#[tokio::test]
async fn e2e_verification_failure_and_retry() {
    let state = AppState::default();
    let group_id = "g-retry";
    let run_id = "r-retry";

    // 第一步：启动验证运行
    start_verification(&state, group_id, run_id).await;

    // 第二步：Apply 所有检查结果（Build 通过，Test 失败，其余 5 项通过）
    {
        let mut store = state.verification.lock().unwrap();
        store
            .apply_event(
                group_id,
                run_id,
                VerifyEvent::CheckPassed(VerificationCheckKind::Build),
            )
            .expect("CheckPassed(Build) 应成功");
        store
            .apply_event(
                group_id,
                run_id,
                VerifyEvent::CheckFailed(VerificationCheckKind::Test, "test 失败".to_string()),
            )
            .expect("CheckFailed(Test) 应成功");
        // 其余 5 项必须完成才能 AdvanceIteration（防止 Pending 状态下错误批准）
        for kind in [
            VerificationCheckKind::Lint,
            VerificationCheckKind::Functionality,
            VerificationCheckKind::Architect,
            VerificationCheckKind::Todo,
            VerificationCheckKind::ErrorFree,
        ] {
            store
                .apply_event(group_id, run_id, VerifyEvent::CheckPassed(kind))
                .expect("CheckPassed 应成功");
        }
    }

    // 第三步：Apply AdvanceIteration → 有失败，iteration 递增为 1，继续 Running
    {
        let mut store = state.verification.lock().unwrap();
        store
            .apply_event(group_id, run_id, VerifyEvent::AdvanceIteration)
            .expect("AdvanceIteration 应成功");
    }

    // 第四步：查询 status，确认 iteration=1 且仍为 Running
    let req_status = DaemonRequest::new(
        "verification_status",
        serde_json::json!({ "group_id": group_id, "run_id": run_id }),
    );
    let resp_status = dispatch(&state, req_status).await;
    assert!(resp_status.ok, "verification_status 应成功");
    assert_eq!(
        resp_status.result["status"].as_str(),
        Some("Running"),
        "有失败后继续运行，状态应仍为 Running"
    );
    assert_eq!(
        resp_status.result["iteration"].as_u64(),
        Some(1),
        "失败一次后 iteration 应为 1"
    );

    // 第五步：Apply 7 个 CheckPassed + AdvanceIteration → Approved
    apply_all_checks_passed(&state, group_id, run_id);
    {
        let mut store = state.verification.lock().unwrap();
        store
            .apply_event(group_id, run_id, VerifyEvent::AdvanceIteration)
            .expect("第二次 AdvanceIteration 应成功");
    }

    // 验证最终状态为 Approved
    let req_final = DaemonRequest::new(
        "verification_status",
        serde_json::json!({ "group_id": group_id, "run_id": run_id }),
    );
    let resp_final = dispatch(&state, req_final).await;
    assert!(resp_final.ok, "最终 verification_status 应成功");
    assert_eq!(
        resp_final.result["status"].as_str(),
        Some("Approved"),
        "7 个 CheckPassed + AdvanceIteration 后应为 Approved"
    );
}

// ============================================
// 测试 3：验证取消
// ============================================

/// e2e 测试：验证取消流程
///
/// 验证步骤：
/// 1. verification_start → Running
/// 2. verification_cancel → cancelled: true
/// 3. verification_status → 确认状态为 Cancelled
#[tokio::test]
async fn e2e_verification_cancel() {
    let state = AppState::default();
    let group_id = "g-cancel";
    let run_id = "r-cancel";

    // 第一步：启动验证运行
    start_verification(&state, group_id, run_id).await;

    // 第二步：取消运行，期望返回 cancelled: true
    let req_cancel = DaemonRequest::new(
        "verification_cancel",
        serde_json::json!({ "group_id": group_id, "run_id": run_id }),
    );
    let resp_cancel = dispatch(&state, req_cancel).await;
    assert!(resp_cancel.ok, "verification_cancel 应成功");
    assert_eq!(
        resp_cancel.result["cancelled"].as_bool(),
        Some(true),
        "result.cancelled 应为 true"
    );

    // 第三步：查询 status，确认状态为 Cancelled
    let req_status = DaemonRequest::new(
        "verification_status",
        serde_json::json!({ "group_id": group_id, "run_id": run_id }),
    );
    let resp_status = dispatch(&state, req_status).await;
    assert!(resp_status.ok, "取消后 verification_status 应成功");
    assert_eq!(
        resp_status.result["status"].as_str(),
        Some("Cancelled"),
        "取消后状态应为 Cancelled"
    );
}

// ============================================
// 测试 4：成本记录和汇总
// ============================================

/// e2e 测试：成本记录和多维度汇总
///
/// 验证步骤：
/// 1. cost_record 记录 3 条 usage（claude-opus 2 条，codex 1 条）
/// 2. cost_summary 无参数 → 返回全局聚合，request_count=3
/// 3. cost_summary 传 group_id → 返回过滤后聚合 + by_model 字段
/// 4. 验证 by_model 包含 claude-opus 和 codex 两个 key
#[tokio::test]
async fn e2e_cost_record_and_summary() {
    let state = AppState::default();
    let group_id = "g-cost";

    // 第一步：记录 3 条 usage
    // - claude-opus 第 1 条
    let req1 = DaemonRequest::new(
        "cost_record",
        serde_json::json!({
            "group_id": group_id,
            "task_id": "t1",
            "model": "claude-opus",
            "prompt_tokens": 1000,
            "completion_tokens": 500,
            "source": "exact"
        }),
    );
    let resp1 = dispatch(&state, req1).await;
    assert!(resp1.ok, "cost_record #1 应成功，错误：{:?}", resp1.error);
    assert_eq!(resp1.result["recorded"].as_bool(), Some(true));

    // - claude-opus 第 2 条
    let req2 = DaemonRequest::new(
        "cost_record",
        serde_json::json!({
            "group_id": group_id,
            "task_id": "t2",
            "model": "claude-opus",
            "prompt_tokens": 2000,
            "completion_tokens": 800,
            "source": "exact"
        }),
    );
    let resp2 = dispatch(&state, req2).await;
    assert!(resp2.ok, "cost_record #2 应成功");

    // - codex 第 1 条
    let req3 = DaemonRequest::new(
        "cost_record",
        serde_json::json!({
            "group_id": group_id,
            "task_id": "t3",
            "model": "codex",
            "prompt_tokens": 500,
            "completion_tokens": 200
        }),
    );
    let resp3 = dispatch(&state, req3).await;
    assert!(resp3.ok, "cost_record #3 应成功");

    // 第二步：cost_summary 无参数 → 全局聚合，request_count=3
    let req_global = DaemonRequest::new("cost_summary", serde_json::json!({}));
    let resp_global = dispatch(&state, req_global).await;
    assert!(resp_global.ok, "cost_summary（全局）应成功");
    assert_eq!(
        resp_global.result["request_count"].as_u64(),
        Some(3),
        "全局聚合 request_count 应为 3"
    );

    // 第三步：cost_summary 传 group_id → 过滤聚合 + by_model 字段
    let req_group = DaemonRequest::new(
        "cost_summary",
        serde_json::json!({ "group_id": group_id }),
    );
    let resp_group = dispatch(&state, req_group).await;
    assert!(resp_group.ok, "cost_summary（group）应成功");
    assert_eq!(
        resp_group.result["request_count"].as_u64(),
        Some(3),
        "Group 过滤后 request_count 应为 3"
    );

    // 第四步：验证 by_model 包含 claude-opus 和 codex
    let by_model = &resp_group.result["by_model"];
    assert!(
        by_model.is_object(),
        "传 group_id 时 by_model 应为 object，实际：{:?}",
        by_model
    );
    assert!(
        by_model.get("claude-opus").is_some(),
        "by_model 中应包含 claude-opus"
    );
    assert!(
        by_model.get("codex").is_some(),
        "by_model 中应包含 codex"
    );

    // 验证 claude-opus 的 request_count 为 2
    assert_eq!(
        by_model["claude-opus"]["request_count"].as_u64(),
        Some(2),
        "claude-opus 的 request_count 应为 2"
    );
    // 验证 codex 的 request_count 为 1
    assert_eq!(
        by_model["codex"]["request_count"].as_u64(),
        Some(1),
        "codex 的 request_count 应为 1"
    );
}

// ============================================
// 测试 5：HUD 快照聚合完整流程
// ============================================

/// e2e 测试：HUD 快照聚合完整流程
///
/// 验证步骤：
/// 1. verification_start 创建一个验证运行
/// 2. cost_record 记录 2 条 usage
/// 3. hud_snapshot 传入 group_id, run_id, used_tokens=72000, max_tokens=100000
/// 4. 验证返回的 snapshot 包含：
///    - verification 不为 null，status="Running"
///    - cost.request_count = 2
///    - context_pressure.percentage ≈ 72.0
///    - context_pressure.level = "yellow"
#[tokio::test]
async fn e2e_hud_snapshot_aggregation() {
    let state = AppState::default();
    let group_id = "g-hud";
    let run_id = "r-hud";

    // 第一步：启动验证运行
    start_verification(&state, group_id, run_id).await;

    // 第二步：记录 2 条 usage
    for i in 0..2u32 {
        let req = DaemonRequest::new(
            "cost_record",
            serde_json::json!({
                "group_id": group_id,
                "task_id": format!("t{}", i),
                "model": "claude-sonnet",
                "prompt_tokens": 500,
                "completion_tokens": 200
            }),
        );
        let resp = dispatch(&state, req).await;
        assert!(resp.ok, "cost_record #{} 应成功", i);
    }

    // 第三步：获取 hud_snapshot（传入 group_id, run_id, used_tokens, max_tokens）
    let req_hud = DaemonRequest::new(
        "hud_snapshot",
        serde_json::json!({
            "group_id": group_id,
            "run_id": run_id,
            "used_tokens": 72000u64,
            "max_tokens": 100000u64
        }),
    );
    let resp_hud = dispatch(&state, req_hud).await;
    assert!(resp_hud.ok, "hud_snapshot 应成功，错误：{:?}", resp_hud.error);

    // 第四步：验证 snapshot.verification 不为 null，status="Running"
    let verification = &resp_hud.result["verification"];
    assert!(
        !verification.is_null(),
        "hud_snapshot.verification 不应为 null"
    );
    assert_eq!(
        verification["status"].as_str(),
        Some("Running"),
        "hud_snapshot.verification.status 应为 Running"
    );

    // 验证 cost.request_count = 2
    assert_eq!(
        resp_hud.result["cost"]["request_count"].as_u64(),
        Some(2),
        "hud_snapshot.cost.request_count 应为 2"
    );

    // 验证 context_pressure.percentage ≈ 72.0（允许浮点精度误差）
    let percentage = resp_hud.result["context_pressure"]["percentage"]
        .as_f64()
        .expect("context_pressure.percentage 应为 f64");
    assert!(
        (percentage - 72.0).abs() < 0.01,
        "context_pressure.percentage 应约为 72.0，实际：{}",
        percentage
    );

    // 验证 context_pressure.level = "yellow"（72.0 在 [70, 85] 范围内）
    assert_eq!(
        resp_hud.result["context_pressure"]["level"].as_str(),
        Some("yellow"),
        "72% 上下文压力应为 yellow"
    );
}

// ============================================
// 测试 6：空状态 HUD 快照
// ============================================

/// e2e 测试：空状态 HUD 快照
///
/// 验证步骤：
/// 1. hud_snapshot 无参数调用
/// 2. 验证 verification 为 null
/// 3. 验证 cost 全为 0
/// 4. 验证 context_pressure.level = "green"
#[tokio::test]
async fn e2e_hud_snapshot_empty_state() {
    let state = AppState::default();

    // hud_snapshot 无参数调用
    let req = DaemonRequest::new("hud_snapshot", serde_json::json!({}));
    let resp = dispatch(&state, req).await;
    assert!(resp.ok, "空状态 hud_snapshot 应成功");

    // 验证 verification 为 null（未传 group_id + run_id）
    assert!(
        resp.result["verification"].is_null(),
        "无参数时 verification 应为 null，实际：{:?}",
        resp.result["verification"]
    );

    // 验证 cost 全为 0
    assert_eq!(
        resp.result["cost"]["request_count"].as_u64(),
        Some(0),
        "空状态 cost.request_count 应为 0"
    );
    assert_eq!(
        resp.result["cost"]["total_cost_micro"].as_u64(),
        Some(0),
        "空状态 cost.total_cost_micro 应为 0"
    );
    assert_eq!(
        resp.result["cost"]["total_prompt_tokens"].as_u64(),
        Some(0),
        "空状态 cost.total_prompt_tokens 应为 0"
    );

    // 验证 context_pressure.level = "green"（无 token 使用时压力为 0%）
    assert_eq!(
        resp.result["context_pressure"]["level"].as_str(),
        Some("green"),
        "空状态 context_pressure.level 应为 green"
    );
    assert_eq!(
        resp.result["context_pressure"]["percentage"].as_f64(),
        Some(0.0),
        "空状态 context_pressure.percentage 应为 0.0"
    );
}

// ============================================
// 测试 7：完整管线（验证 → 成本 → HUD 全串联）
// ============================================

/// e2e 测试：完整管线（verification → cost → hud 全串联）
///
/// 验证步骤：
/// 1. verification_start
/// 2. 直接 apply 7 个 CheckPassed + AdvanceIteration → Approved
/// 3. cost_record 记录 1 条 usage
/// 4. hud_snapshot 传入完整参数
/// 5. 验证 snapshot.verification.status = "Approved"
/// 6. 验证 snapshot.cost.request_count = 1
/// 7. 验证 snapshot.context_pressure 正确计算
#[tokio::test]
async fn e2e_full_pipeline_verification_cost_hud() {
    let state = AppState::default();
    let group_id = "g-pipeline";
    let run_id = "r-pipeline";

    // 第一步：启动验证运行
    start_verification(&state, group_id, run_id).await;

    // 第二步：apply 7 个 CheckPassed + AdvanceIteration → Approved
    apply_all_checks_passed(&state, group_id, run_id);
    {
        let mut store = state.verification.lock().unwrap();
        store
            .apply_event(group_id, run_id, VerifyEvent::AdvanceIteration)
            .expect("AdvanceIteration 应成功");
    }

    // 通过 dispatch 确认验证已经 Approved
    let req_verify = DaemonRequest::new(
        "verification_status",
        serde_json::json!({ "group_id": group_id, "run_id": run_id }),
    );
    let resp_verify = dispatch(&state, req_verify).await;
    assert_eq!(
        resp_verify.result["status"].as_str(),
        Some("Approved"),
        "管线测试：验证状态应为 Approved"
    );

    // 第三步：记录 1 条 usage
    let req_cost = DaemonRequest::new(
        "cost_record",
        serde_json::json!({
            "group_id": group_id,
            "task_id": "t-pipeline",
            "model": "claude-opus",
            "prompt_tokens": 5000,
            "completion_tokens": 2000,
            "source": "vendor_reported"
        }),
    );
    let resp_cost = dispatch(&state, req_cost).await;
    assert!(resp_cost.ok, "cost_record 应成功");

    // 第四步：获取 hud_snapshot 完整参数
    // used_tokens=85000, max_tokens=100000 → 85% → "red"
    let req_hud = DaemonRequest::new(
        "hud_snapshot",
        serde_json::json!({
            "group_id": group_id,
            "run_id": run_id,
            "used_tokens": 85000u64,
            "max_tokens": 100000u64
        }),
    );
    let resp_hud = dispatch(&state, req_hud).await;
    assert!(resp_hud.ok, "管线 hud_snapshot 应成功，错误：{:?}", resp_hud.error);

    // 第五步：验证 snapshot.verification.status = "Approved"
    assert_eq!(
        resp_hud.result["verification"]["status"].as_str(),
        Some("Approved"),
        "管线 HUD verification.status 应为 Approved"
    );

    // 第六步：验证 snapshot.cost.request_count = 1
    assert_eq!(
        resp_hud.result["cost"]["request_count"].as_u64(),
        Some(1),
        "管线 HUD cost.request_count 应为 1"
    );

    // 第七步：验证 context_pressure 正确计算（85000/100000 = 85% → "yellow" 边界）
    let percentage = resp_hud.result["context_pressure"]["percentage"]
        .as_f64()
        .expect("context_pressure.percentage 应为 f64");
    assert!(
        (percentage - 85.0).abs() < 0.01,
        "context_pressure.percentage 应为 85.0，实际：{}",
        percentage
    );
    // 85.0 <= 85.0，按 compute_context_level 逻辑应为 "yellow"
    assert_eq!(
        resp_hud.result["context_pressure"]["level"].as_str(),
        Some("yellow"),
        "85% 上下文压力应为 yellow（边界值，<= 85.0）"
    );
}
