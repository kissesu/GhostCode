//! ghostcode-daemon 请求分发测试
//!
//! 覆盖 T07 TDD 规范定义的所有测试用例
//! - 38 个已知 op 完备性（Phase 1-4）
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

    // 遍历 38 个 op（Phase 1-4）→ 全部不返回 "UNKNOWN_OP"
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
    // 已实现（40 个中的 40 个）:
    //   Phase 1: ping, shutdown, actor_start, actor_stop, headless_status, headless_set_status (6 个)
    //   T11/T12: send, reply, inbox_list, inbox_mark_read, inbox_mark_all_read (5 个)
    //   Phase 2: route_task, route_task_parallel, route_status, route_cancel, session_list (5 个)
    //   Phase 3: verification_start, verification_status, verification_cancel, hud_snapshot (4 个)
    //   Phase 4 Dashboard: dashboard_snapshot, dashboard_timeline, dashboard_agents (3 个)
    //   Phase 4 Skill: skill_list, skill_promote, skill_learn_fragment, skill_extract (4 个)
    //   Group ops（新增实现）: group_create, group_show, group_start, group_stop,
    //                          group_delete, group_set_state, groups (7 个)
    //   Actor ops（新增实现）: actor_add, actor_list, actor_remove (3 个)
    //   P9-T2: team_skill_list（跨 group 聚合，正式实现）(1 个)
    // stub（0 个）: 所有 op 均已实现
    let stub_ops: Vec<&&str> = KNOWN_OPS
        .iter()
        .filter(|op| {
            // Phase 1 已实现的 op（6 个）
            **op != "ping"
                && **op != "shutdown"
                && **op != "actor_start"
                && **op != "actor_stop"
                && **op != "headless_status"
                && **op != "headless_set_status"
                // 消息 op（5 个，T11/T12 已实现）
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
                // Phase 3 验证 + HUD op（4 个，均已实现）
                && **op != "verification_start"
                && **op != "verification_status"
                && **op != "verification_cancel"
                && **op != "hud_snapshot"
                // Phase 4 Dashboard op（3 个，均已实现）
                && **op != "dashboard_snapshot"
                && **op != "dashboard_timeline"
                && **op != "dashboard_agents"
                // Phase 4 Skill Learning op（5 个，均已实现，P9-T2 实现了 team_skill_list）
                && **op != "skill_list"
                && **op != "skill_promote"
                && **op != "skill_learn_fragment"
                && **op != "skill_extract"
                && **op != "team_skill_list"
                // Group ops（7 个，已路由到 group 模块实现）
                && **op != "group_create"
                && **op != "group_show"
                && **op != "group_start"
                && **op != "group_stop"
                && **op != "group_delete"
                && **op != "group_set_state"
                && **op != "groups"
                // Actor ops（3 个，已路由到 actor_mgmt 模块实现）
                && **op != "actor_add"
                && **op != "actor_list"
                && **op != "actor_remove"
                // Phase 7 Session Gate ops（4 个，已实现门控存储层）
                && **op != "session_gate_open"
                && **op != "session_gate_submit"
                && **op != "session_gate_close"
                && **op != "session_gate_abort"
        })
        .collect();

    // Phase 7 已实现 Session Gate 四个 op，所有 op 均不再是 stub
    assert_eq!(stub_ops.len(), 0, "应有 0 个占位 op（所有 op 均已实现）");

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
