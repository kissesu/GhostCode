//! ghostcode-daemon 请求分发测试
//!
//! 覆盖 T07 TDD 规范定义的所有测试用例
//! - 21 个已知 op 完备性
//! - 未知 op 安全性（PBT）
//! - ping 响应格式 [ERR-3]
//!
//! @author Atlas.oi
//! @date 2026-03-01

use std::sync::Arc;

use ghostcode_daemon::dispatch::{dispatch, KNOWN_OPS};
use ghostcode_daemon::server::AppState;
use ghostcode_types::ipc::DaemonRequest;
use proptest::prelude::*;

// ============================================
// 异步测试
// ============================================

#[tokio::test]
async fn dispatch_all_known_ops() {
    let state = AppState::default();

    // 遍历 21 个 op → 全部不返回 "UNKNOWN_OP"
    for op in KNOWN_OPS {
        let req = DaemonRequest::new(*op, serde_json::json!({}));
        let resp = dispatch(&state, req).await;

        // 不应返回 UNKNOWN_OP 错误
        if let Some(err) = &resp.error {
            assert_ne!(
                err.code, "UNKNOWN_OP",
                "op '{}' 不应返回 UNKNOWN_OP",
                op
            );
        }
    }
}

#[tokio::test]
async fn ping_returns_version() {
    // [ERR-3] ping 响应应包含 version 和 has_unread 字段
    let state = AppState::default();

    let req = DaemonRequest::new("ping", serde_json::json!({}));
    let resp = dispatch(&state, req).await;

    assert!(resp.ok, "ping 应返回成功");

    let result = &resp.result;
    assert_eq!(result["pong"], true, "应包含 pong: true");
    assert!(result.get("version").is_some(), "应包含 version 字段");
    assert!(
        result.get("has_unread").is_some(),
        "应包含 has_unread 字段"
    );
}

#[tokio::test]
async fn shutdown_triggers_shutdown() {
    let state = Arc::new(AppState::default());

    let req = DaemonRequest::new("shutdown", serde_json::json!({}));
    let resp = dispatch(&state, req).await;

    assert!(resp.ok, "shutdown 应返回成功");
    assert_eq!(resp.result["shutting_down"], true);
}

#[tokio::test]
async fn unknown_op_returns_error() {
    let state = AppState::default();

    let req = DaemonRequest::new("nonexistent_operation", serde_json::json!({}));
    let resp = dispatch(&state, req).await;

    assert!(!resp.ok, "未知 op 应返回失败");
    assert_eq!(resp.error.as_ref().unwrap().code, "UNKNOWN_OP");
}

#[tokio::test]
async fn stub_ops_return_not_implemented() {
    let state = AppState::default();

    // 除了已实现的 handler，其余 op 应返回 NOT_IMPLEMENTED
    // 已实现: ping, shutdown, actor_start, actor_stop, headless_status, headless_set_status,
    //         send, reply (T11), inbox_list, inbox_mark_read, inbox_mark_all_read (T12)
    let stub_ops: Vec<&&str> = KNOWN_OPS
        .iter()
        .filter(|op| {
            // Phase 1 已实现的 op（11 个）
            **op != "ping"
                && **op != "shutdown"
                && **op != "actor_start"
                && **op != "actor_stop"
                && **op != "headless_status"
                && **op != "headless_set_status"
                && **op != "send"
                && **op != "reply"
                && **op != "inbox_list"
                && **op != "inbox_mark_read"
                && **op != "inbox_mark_all_read"
                // Phase 2 路由 op（5 个，均已实现）
                && **op != "route_task"
                && **op != "route_task_parallel"
                && **op != "route_status"
                && **op != "route_cancel"
                && **op != "session_list"
        })
        .collect();

    assert_eq!(stub_ops.len(), 10, "应有 10 个占位 op (26 总 - 16 已实现)");

    for op in stub_ops {
        let req = DaemonRequest::new(*op, serde_json::json!({}));
        let resp = dispatch(&state, req).await;

        assert!(!resp.ok, "stub op '{}' 应返回失败", op);
        assert_eq!(
            resp.error.as_ref().unwrap().code,
            "NOT_IMPLEMENTED",
            "stub op '{}' 应返回 NOT_IMPLEMENTED",
            op
        );
    }
}

// ============================================
// PBT：未知 op 安全性
// ============================================

proptest! {
    /// PBT: 任意随机字符串 op 都返回 ok: false（不 panic）
    #[test]
    fn unknown_op_never_panics(op in "[a-z_]{1,50}") {
        // 排除已知 op
        if KNOWN_OPS.contains(&op.as_str()) {
            return Ok(());
        }

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let state = AppState::default();
            let req = DaemonRequest::new(op.clone(), serde_json::json!({}));
            let resp = dispatch(&state, req).await;

            prop_assert!(!resp.ok, "未知 op '{}' 应返回失败", op);
            prop_assert_eq!(
                &resp.error.as_ref().unwrap().code,
                "UNKNOWN_OP",
                "未知 op '{}' 应返回 UNKNOWN_OP",
                op
            );
            Ok(())
        })?;
    }
}
