//! Daemon 脚手架扩展测试（T36，T41，T42 更新）
//!
//! 验证 6 个新 op 的存在性及返回格式：
//! - verification_start → 无参数时返回 INVALID_ARGS（T41 后不再是空壳）
//! - verification_status → 无参数时返回 INVALID_ARGS
//! - verification_cancel → 无参数时返回 INVALID_ARGS
//! - hud_snapshot → ok: true, result: {}
//! - cost_record → 无参数时返回 INVALID_ARGS（T42 后需要 group_id 和 model）
//! - cost_summary → ok: true, result: {}（可选参数，无参数时全局聚合）
//!
//! @author Atlas.oi
//! @date 2026-03-03

use ghostcode_daemon::dispatch::{dispatch, KNOWN_OPS};
use ghostcode_daemon::server::AppState;
use ghostcode_types::ipc::DaemonRequest;

/// 6 个新 op 名称列表，方便统一验证
const NEW_OPS: &[&str] = &[
    "verification_start",
    "verification_status",
    "verification_cancel",
    "hud_snapshot",
    "cost_record",
    "cost_summary",
];

/// T41/T42 之后需要必填参数的 ops，无参数时 ok: false
/// 仅需确认它们不返回 UNKNOWN_OP
const OPS_REQUIRING_ARGS: &[&str] = &[
    "verification_start",
    "verification_status",
    "verification_cancel",
    "cost_record",
];

// ============================================
// 验证新 op 在 KNOWN_OPS 中注册
// ============================================

#[test]
fn new_ops_registered_in_known_ops() {
    // 确认 6 个新 op 都已加入 KNOWN_OPS 常量
    for op in NEW_OPS {
        assert!(
            KNOWN_OPS.contains(op),
            "op '{}' 应在 KNOWN_OPS 中注册",
            op
        );
    }
}

// ============================================
// 验证新 op 调用结果格式
// ============================================

/// T41 更新：verification_start 现在需要必填参数，空参数应返回 INVALID_ARGS
#[tokio::test]
async fn verification_start_requires_args() {
    let state = AppState::default();
    let req = DaemonRequest::new("verification_start", serde_json::json!({}));
    let resp = dispatch(&state, req).await;

    // verification_start 现在是有参数验证的实际实现，无参数应返回 INVALID_ARGS
    assert!(!resp.ok, "无参数的 verification_start 应返回 ok: false");
    assert_eq!(
        resp.error.as_ref().unwrap().code,
        "INVALID_ARGS",
        "无参数的 verification_start 错误码应为 INVALID_ARGS"
    );
}

/// T41 更新：verification_status 现在需要必填参数，空参数应返回 INVALID_ARGS
#[tokio::test]
async fn verification_status_requires_args() {
    let state = AppState::default();
    let req = DaemonRequest::new("verification_status", serde_json::json!({}));
    let resp = dispatch(&state, req).await;

    assert!(!resp.ok, "无参数的 verification_status 应返回 ok: false");
    assert_eq!(
        resp.error.as_ref().unwrap().code,
        "INVALID_ARGS",
        "无参数的 verification_status 错误码应为 INVALID_ARGS"
    );
}

/// T41 更新：verification_cancel 现在需要必填参数，空参数应返回 INVALID_ARGS
#[tokio::test]
async fn verification_cancel_requires_args() {
    let state = AppState::default();
    let req = DaemonRequest::new("verification_cancel", serde_json::json!({}));
    let resp = dispatch(&state, req).await;

    assert!(!resp.ok, "无参数的 verification_cancel 应返回 ok: false");
    assert_eq!(
        resp.error.as_ref().unwrap().code,
        "INVALID_ARGS",
        "无参数的 verification_cancel 错误码应为 INVALID_ARGS"
    );
}

#[tokio::test]
async fn hud_snapshot_returns_ok_empty_result() {
    let state = AppState::default();
    let req = DaemonRequest::new("hud_snapshot", serde_json::json!({}));
    let resp = dispatch(&state, req).await;

    assert!(resp.ok, "hud_snapshot 应返回 ok: true");
    assert!(resp.error.is_none(), "hud_snapshot 不应有 error");
    assert!(resp.result.is_object(), "hud_snapshot result 应为 JSON 对象");
}

/// T42 更新：cost_record 现在需要必填参数（group_id、model），空参数应返回 INVALID_ARGS
#[tokio::test]
async fn cost_record_requires_args() {
    let state = AppState::default();
    let req = DaemonRequest::new("cost_record", serde_json::json!({}));
    let resp = dispatch(&state, req).await;

    // cost_record 现在是有参数验证的实际实现，无参数应返回 INVALID_ARGS
    assert!(!resp.ok, "无参数的 cost_record 应返回 ok: false");
    assert_eq!(
        resp.error.as_ref().unwrap().code,
        "INVALID_ARGS",
        "无参数的 cost_record 错误码应为 INVALID_ARGS"
    );
}

#[tokio::test]
async fn cost_summary_returns_ok_empty_result() {
    let state = AppState::default();
    let req = DaemonRequest::new("cost_summary", serde_json::json!({}));
    let resp = dispatch(&state, req).await;

    assert!(resp.ok, "cost_summary 应返回 ok: true");
    assert!(resp.error.is_none(), "cost_summary 不应有 error");
    assert!(resp.result.is_object(), "cost_summary result 应为 JSON 对象");
}

// ============================================
// 批量验证：所有新 op 均可调用且不返回 UNKNOWN_OP
// ============================================

#[tokio::test]
async fn all_new_ops_callable_no_unknown_op() {
    let state = AppState::default();

    for op in NEW_OPS {
        let req = DaemonRequest::new(*op, serde_json::json!({}));
        let resp = dispatch(&state, req).await;

        // 所有 op 都不应返回 UNKNOWN_OP
        if let Some(err) = &resp.error {
            assert_ne!(
                err.code, "UNKNOWN_OP",
                "新 op '{}' 不应返回 UNKNOWN_OP",
                op
            );
        }

        // T41/T42 后部分 ops 需要必填参数，空参数下允许 ok: false
        // 其余 ops（hud_snapshot、cost_summary）仍应返回 ok: true
        if !OPS_REQUIRING_ARGS.contains(op) {
            assert!(resp.ok, "op '{}' 应返回 ok: true", op);
        }
    }
}
