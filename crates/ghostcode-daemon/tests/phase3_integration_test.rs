//! Phase 3 端到端集成测试
//!
//! @file phase3_integration_test.rs
//! @description 覆盖 Phase 3（验证、HUD）所有模块的端到端集成测试
//!              测试验证生命周期、HUD 快照的完整业务流程链路：
//!              - verification 生命周期（启动 → 推进 → 批准/取消/失败重试）
//!              - hud_snapshot 全链路聚合
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

#[tokio::test]
async fn e2e_verification_full_lifecycle() {
    let state = AppState::default();
    let group_id = "g-lifecycle";
    let run_id = "r-lifecycle";

    let req_start = DaemonRequest::new(
        "verification_start",
        serde_json::json!({ "group_id": group_id, "run_id": run_id }),
    );
    let resp_start = dispatch(&state, req_start).await;
    assert!(resp_start.ok, "verification_start 应成功");
    assert_eq!(resp_start.result["status"].as_str(), Some("Running"));
    assert_eq!(resp_start.result["iteration"].as_u64(), Some(0));

    apply_all_checks_passed(&state, group_id, run_id);
    {
        let mut store = state.verification.lock().unwrap();
        store
            .apply_event(group_id, run_id, VerifyEvent::AdvanceIteration)
            .expect("AdvanceIteration 应成功");
    }

    let req_status = DaemonRequest::new(
        "verification_status",
        serde_json::json!({ "group_id": group_id, "run_id": run_id }),
    );
    let resp_status = dispatch(&state, req_status).await;
    assert!(resp_status.ok, "verification_status 应成功");
    assert_eq!(resp_status.result["status"].as_str(), Some("Approved"));
    assert_eq!(resp_status.result["iteration"].as_u64(), Some(0));
}

// ============================================
// 测试 2：验证失败和重试
// ============================================

#[tokio::test]
async fn e2e_verification_failure_and_retry() {
    let state = AppState::default();
    let group_id = "g-retry";
    let run_id = "r-retry";

    start_verification(&state, group_id, run_id).await;

    {
        let mut store = state.verification.lock().unwrap();
        store
            .apply_event(group_id, run_id, VerifyEvent::CheckPassed(VerificationCheckKind::Build))
            .expect("CheckPassed(Build) 应成功");
        store
            .apply_event(group_id, run_id, VerifyEvent::CheckFailed(VerificationCheckKind::Test, "test 失败".to_string()))
            .expect("CheckFailed(Test) 应成功");
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

    {
        let mut store = state.verification.lock().unwrap();
        store
            .apply_event(group_id, run_id, VerifyEvent::AdvanceIteration)
            .expect("AdvanceIteration 应成功");
    }

    let req_status = DaemonRequest::new(
        "verification_status",
        serde_json::json!({ "group_id": group_id, "run_id": run_id }),
    );
    let resp_status = dispatch(&state, req_status).await;
    assert!(resp_status.ok);
    assert_eq!(resp_status.result["status"].as_str(), Some("Running"));
    assert_eq!(resp_status.result["iteration"].as_u64(), Some(1));

    apply_all_checks_passed(&state, group_id, run_id);
    {
        let mut store = state.verification.lock().unwrap();
        store
            .apply_event(group_id, run_id, VerifyEvent::AdvanceIteration)
            .expect("第二次 AdvanceIteration 应成功");
    }

    let req_final = DaemonRequest::new(
        "verification_status",
        serde_json::json!({ "group_id": group_id, "run_id": run_id }),
    );
    let resp_final = dispatch(&state, req_final).await;
    assert!(resp_final.ok);
    assert_eq!(resp_final.result["status"].as_str(), Some("Approved"));
}

// ============================================
// 测试 3：验证取消
// ============================================

#[tokio::test]
async fn e2e_verification_cancel() {
    let state = AppState::default();
    let group_id = "g-cancel";
    let run_id = "r-cancel";

    start_verification(&state, group_id, run_id).await;

    let req_cancel = DaemonRequest::new(
        "verification_cancel",
        serde_json::json!({ "group_id": group_id, "run_id": run_id }),
    );
    let resp_cancel = dispatch(&state, req_cancel).await;
    assert!(resp_cancel.ok, "verification_cancel 应成功");
    assert_eq!(resp_cancel.result["cancelled"].as_bool(), Some(true));

    let req_status = DaemonRequest::new(
        "verification_status",
        serde_json::json!({ "group_id": group_id, "run_id": run_id }),
    );
    let resp_status = dispatch(&state, req_status).await;
    assert!(resp_status.ok);
    assert_eq!(resp_status.result["status"].as_str(), Some("Cancelled"));
}

// ============================================
// 测试 4：HUD 快照聚合（含验证）
// ============================================

#[tokio::test]
async fn e2e_hud_snapshot_aggregation() {
    let state = AppState::default();
    let group_id = "g-hud";
    let run_id = "r-hud";

    start_verification(&state, group_id, run_id).await;

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

    // 验证摘要存在，status="Running"
    let verification = &resp_hud.result["verification"];
    assert!(!verification.is_null(), "hud_snapshot.verification 不应为 null");
    assert_eq!(verification["status"].as_str(), Some("Running"));

    // context_pressure ≈ 72.0, level="yellow"
    let percentage = resp_hud.result["context_pressure"]["percentage"]
        .as_f64()
        .expect("context_pressure.percentage 应为 f64");
    assert!((percentage - 72.0).abs() < 0.01);
    assert_eq!(resp_hud.result["context_pressure"]["level"].as_str(), Some("yellow"));
}

// ============================================
// 测试 5：空状态 HUD 快照
// ============================================

#[tokio::test]
async fn e2e_hud_snapshot_empty_state() {
    let state = AppState::default();

    let req = DaemonRequest::new("hud_snapshot", serde_json::json!({}));
    let resp = dispatch(&state, req).await;
    assert!(resp.ok, "空状态 hud_snapshot 应成功");

    // verification 为 null
    assert!(resp.result["verification"].is_null());

    // context_pressure.level = "green"
    assert_eq!(resp.result["context_pressure"]["level"].as_str(), Some("green"));
    assert_eq!(resp.result["context_pressure"]["percentage"].as_f64(), Some(0.0));
}
