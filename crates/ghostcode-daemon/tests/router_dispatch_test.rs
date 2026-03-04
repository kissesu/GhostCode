//! router_dispatch_test.rs
//!
//! T30 路由 Dispatch 集成测试
//! 通过直接调用 dispatch() 函数验证 5 个新路由 op 的行为
//! 不需要启动真实 Unix Socket Server
//!
//! @author Atlas.oi
//! @date 2026-03-02

use ghostcode_daemon::dispatch::dispatch;
use ghostcode_daemon::server::AppState;
use ghostcode_types::ipc::DaemonRequest;

// ============================================
// 辅助函数：构造 DaemonRequest
// ============================================

/// 构造测试用 DaemonRequest
fn make_req(op: &str, args: serde_json::Value) -> DaemonRequest {
    DaemonRequest::new(op, args)
}

// ============================================
// T1: route_task 返回 task_id
// ============================================

/// 验证 route_task op 返回 ok 响应并包含 task_id
#[tokio::test]
async fn route_task_returns_task_id() {
    let state = AppState::default();

    let req = make_req(
        "route_task",
        serde_json::json!({
            "group_id": "test-group",
            "task_text": "实现一个排序算法",
            "backend": "claude"
        }),
    );

    let resp = dispatch(&state, req).await;

    // 断言响应成功
    assert!(resp.ok, "route_task 应返回 ok=true，实际: {:?}", resp);
    // 断言包含 task_id 字段
    assert!(
        resp.result.get("task_id").is_some(),
        "响应应包含 task_id 字段，实际 result: {:?}",
        resp.result
    );
    // 断言包含 backend 字段
    assert!(
        resp.result.get("backend").is_some(),
        "响应应包含 backend 字段，实际 result: {:?}",
        resp.result
    );
}

// ============================================
// T2: route_status 返回任务状态
// ============================================

/// 先 route_task 获取 task_id，再 route_status 查询状态
#[tokio::test]
async fn route_status_returns_state() {
    let state = AppState::default();

    // 第一步：创建任务
    let create_req = make_req(
        "route_task",
        serde_json::json!({
            "group_id": "test-group",
            "task_text": "生成单元测试",
            "backend": "claude"
        }),
    );
    let create_resp = dispatch(&state, create_req).await;
    assert!(create_resp.ok, "route_task 应成功");

    let task_id = create_resp.result["task_id"]
        .as_str()
        .expect("task_id 应为字符串");

    // 第二步：查询状态
    let status_req = make_req(
        "route_status",
        serde_json::json!({
            "group_id": "test-group",
            "task_id": task_id
        }),
    );
    let status_resp = dispatch(&state, status_req).await;

    assert!(status_resp.ok, "route_status 应返回 ok=true");
    assert!(
        status_resp.result.get("task_id").is_some(),
        "状态响应应包含 task_id"
    );
    assert!(
        status_resp.result.get("status").is_some(),
        "状态响应应包含 status 字段"
    );
}

// ============================================
// T3: route_cancel 标记任务为 cancelled
// ============================================

/// route_cancel 后再 route_status 应看到 status=cancelled
#[tokio::test]
async fn route_cancel_marks_cancelled() {
    let state = AppState::default();

    // 创建任务
    let create_resp = dispatch(
        &state,
        make_req(
            "route_task",
            serde_json::json!({
                "group_id": "grp1",
                "task_text": "重构数据库层",
            }),
        ),
    )
    .await;
    assert!(create_resp.ok);

    let task_id = create_resp.result["task_id"]
        .as_str()
        .expect("task_id 应为字符串");

    // 取消任务
    let cancel_resp = dispatch(
        &state,
        make_req(
            "route_cancel",
            serde_json::json!({
                "group_id": "grp1",
                "task_id": task_id
            }),
        ),
    )
    .await;
    assert!(cancel_resp.ok, "route_cancel 应返回 ok=true");
    assert_eq!(
        cancel_resp.result["cancelled"], true,
        "cancelled 字段应为 true"
    );

    // 验证状态已更新为 cancelled
    let status_resp = dispatch(
        &state,
        make_req(
            "route_status",
            serde_json::json!({
                "group_id": "grp1",
                "task_id": task_id
            }),
        ),
    )
    .await;
    assert!(status_resp.ok);
    assert_eq!(
        status_resp.result["status"].as_str().unwrap_or(""),
        "cancelled",
        "status 应为 cancelled"
    );
}

// ============================================
// T4: session_list 返回空列表
// ============================================

/// session_list 当前返回空 sessions 列表（SessionStore 集成在后续迭代完成）
#[tokio::test]
async fn session_list_returns_empty() {
    let state = AppState::default();

    let resp = dispatch(
        &state,
        make_req(
            "session_list",
            serde_json::json!({
                "group_id": "test-group"
            }),
        ),
    )
    .await;

    assert!(resp.ok, "session_list 应返回 ok=true");
    assert!(
        resp.result["sessions"].is_array(),
        "sessions 应为数组"
    );
    assert_eq!(
        resp.result["sessions"].as_array().unwrap().len(),
        0,
        "sessions 当前应为空列表"
    );
}

// ============================================
// T5: route_task_parallel 接受 ---TASK--- 格式
// ============================================

/// route_task_parallel 接受标准 ---TASK---/---CONTENT--- 格式文本
#[tokio::test]
async fn route_task_parallel_accepts_format() {
    let state = AppState::default();

    // 构造合法的并行任务格式文本
    let tasks_format = "\
---TASK---
id: task1
backend: codex
---CONTENT---
实现登录接口
---TASK---
id: task2
backend: codex
dependencies: task1
---CONTENT---
实现注册接口
";

    let resp = dispatch(
        &state,
        make_req(
            "route_task_parallel",
            serde_json::json!({
                "group_id": "test-group",
                "tasks_format": tasks_format
            }),
        ),
    )
    .await;

    assert!(resp.ok, "route_task_parallel 应返回 ok=true，错误: {:?}", resp);
    assert!(
        resp.result.get("task_id").is_some(),
        "响应应包含 task_id"
    );
    assert_eq!(
        resp.result["task_count"].as_u64().unwrap_or(0),
        2,
        "task_count 应为 2"
    );
}

// ============================================
// T6: 非法 group_id 被拒绝
// ============================================

/// 包含 ../ 的 group_id 应返回 INVALID_ARGS 错误（路径遍历防护）
#[tokio::test]
async fn invalid_group_id_rejected() {
    let state = AppState::default();

    let resp = dispatch(
        &state,
        make_req(
            "route_task",
            serde_json::json!({
                "group_id": "../etc/passwd",
                "task_text": "恶意请求"
            }),
        ),
    )
    .await;

    assert!(!resp.ok, "非法 group_id 应返回错误");
    let error_code = resp.error.as_ref().map(|e| e.code.as_str());
    assert_eq!(
        error_code,
        Some("INVALID_ARGS"),
        "错误码应为 INVALID_ARGS，实际: {:?}",
        error_code
    );
}

// ============================================
// T7: 已有 op 不受影响（ping 正常工作）
// ============================================

/// 验证新增 op 不影响已有功能，ping 应正常返回 pong
#[tokio::test]
async fn existing_ops_still_work() {
    let state = AppState::default();

    let resp = dispatch(
        &state,
        make_req("ping", serde_json::json!({})),
    )
    .await;

    assert!(resp.ok, "ping 应返回 ok=true");
    assert_eq!(
        resp.result["pong"].as_bool(),
        Some(true),
        "ping 应返回 pong=true"
    );
}

// ============================================
// T8: 未知 op 返回 UNKNOWN_OP 错误
// ============================================

/// 未注册的 op 应返回 UNKNOWN_OP 错误码
#[tokio::test]
async fn unknown_op_returns_error() {
    let state = AppState::default();

    let resp = dispatch(
        &state,
        make_req("not_a_real_op_xyz", serde_json::json!({})),
    )
    .await;

    assert!(!resp.ok, "未知 op 应返回错误");
    let error_code = resp.error.as_ref().map(|e| e.code.as_str());
    assert_eq!(
        error_code,
        Some("UNKNOWN_OP"),
        "错误码应为 UNKNOWN_OP"
    );
}

// ============================================
// T9: session_list 从新 store 返回空列表
// ============================================

/// 使用新创建的 AppState（包含真实 SessionStore）
/// session_list 应返回空列表
#[tokio::test]
async fn session_list_returns_empty_for_fresh_store() {
    use tempfile::TempDir;
    let tmp = TempDir::new().unwrap();
    // 使用真实 TempDir 初始化 AppState，触发 SessionStore 初始化
    let state = AppState::new(tmp.path().to_path_buf());

    let resp = dispatch(
        &state,
        make_req(
            "session_list",
            serde_json::json!({ "group_id": "test-group" }),
        ),
    )
    .await;

    assert!(resp.ok, "session_list 应返回 ok=true，实际: {:?}", resp);
    assert!(
        resp.result["sessions"].is_array(),
        "sessions 应为数组，实际: {:?}",
        resp.result
    );
    // 全新 SessionStore 没有任何持久化数据，应返回空列表
    assert_eq!(
        resp.result["sessions"].as_array().unwrap().len(),
        0,
        "全新 SessionStore 的 sessions 应为空列表"
    );
}

// ============================================
// T11: session_list 按 group_id 过滤真实 SessionStore 数据
// ============================================

/// 辅助函数：创建含预置 session 数据的 AppState
///
/// 向 session_store 注入 2 个 group 的 session 数据，
/// 用于验证 session_list op 的 group_id 过滤功能
fn make_state_with_session_store() -> (tempfile::TempDir, AppState) {
    use ghostcode_router::session::SessionKey;
    let tmp = tempfile::TempDir::new().expect("创建临时目录失败");
    let state = AppState::new(tmp.path().to_path_buf());

    // grp1: actor1/claude + actor2/openai
    let k1: SessionKey = ("grp1".to_string(), "actor1".to_string(), "claude".to_string());
    let k2: SessionKey = ("grp1".to_string(), "actor2".to_string(), "openai".to_string());
    // grp2: actor3/codex（属于另一个 group，不应出现在 grp1 的查询结果中）
    let k3: SessionKey = ("grp2".to_string(), "actor3".to_string(), "codex".to_string());

    state.session_store.save(k1, "session-id-claude-001".to_string()).expect("save k1 失败");
    state.session_store.save(k2, "session-id-openai-002".to_string()).expect("save k2 失败");
    state.session_store.save(k3, "session-id-codex-003".to_string()).expect("save k3 失败");

    (tmp, state)
}

/// 验证 session_list 按 group_id 正确过滤 SessionStore 数据
///
/// grp1 有 2 条 session，grp2 有 1 条；
/// 查询 grp1 只应返回属于 grp1 的 2 条记录，不包含 grp2 的数据
#[tokio::test]
async fn session_list_filters_by_group_id() {
    let (_tmp, state) = make_state_with_session_store();

    // 查询 grp1 的 sessions
    let resp = dispatch(
        &state,
        make_req("session_list", serde_json::json!({ "group_id": "grp1" })),
    )
    .await;

    assert!(resp.ok, "session_list 应返回 ok=true，错误: {:?}", resp.error);

    let sessions = resp.result["sessions"]
        .as_array()
        .expect("sessions 字段应为数组");

    // grp1 有 2 条 session（actor1/claude + actor2/openai）
    assert_eq!(
        sessions.len(),
        2,
        "grp1 应有 2 条 session，实际: {:?}",
        sessions
    );

    // 验证不包含 grp2 的数据（actor3/codex）
    let has_grp2_data = sessions.iter().any(|s| {
        s["actor_id"].as_str() == Some("actor3") || s["backend"].as_str() == Some("codex")
    });
    assert!(!has_grp2_data, "grp1 的查询结果不应包含 grp2 的数据");
}

/// 验证 session_list 返回的条目包含 actor_id、backend 和 session_id 字段
///
/// 每条记录必须包含三元组信息，供调用方识别 session 归属
#[tokio::test]
async fn session_list_includes_actor_backend_and_session_id() {
    let (_tmp, state) = make_state_with_session_store();

    // 查询 grp1（按 actor_id 排序，actor1 < actor2）
    let resp = dispatch(
        &state,
        make_req("session_list", serde_json::json!({ "group_id": "grp1" })),
    )
    .await;

    assert!(resp.ok, "session_list 应返回 ok=true，错误: {:?}", resp.error);

    let sessions = resp.result["sessions"]
        .as_array()
        .expect("sessions 字段应为数组");

    // 验证每条记录包含必要字段
    for entry in sessions {
        assert!(
            entry.get("actor_id").is_some(),
            "每条 session 记录应包含 actor_id 字段，实际: {}",
            entry
        );
        assert!(
            entry.get("backend").is_some(),
            "每条 session 记录应包含 backend 字段，实际: {}",
            entry
        );
        assert!(
            entry.get("session_id").is_some(),
            "每条 session 记录应包含 session_id 字段，实际: {}",
            entry
        );
    }

    // 验证排序：按 (actor_id, backend) 升序排列
    // actor1/claude 应排在 actor2/openai 之前
    let first_actor = sessions[0]["actor_id"].as_str().unwrap_or("");
    let second_actor = sessions[1]["actor_id"].as_str().unwrap_or("");
    assert!(
        first_actor <= second_actor,
        "sessions 应按 actor_id 升序排列，实际顺序: {} / {}",
        first_actor,
        second_actor
    );
}

// ============================================
// T10: AppState.session_store 字段可读写（SessionStore 注入验证）
// ============================================

/// 验证 AppState.session_store 字段存在且可正常读写：
/// 1. save() 成功存储 session 数据
/// 2. get() 成功读取已存储的 session_id
/// 3. list() 返回正确的 session 条目
///
/// 注意：此测试只验证 AppState.session_store 字段的功能注入，
/// 通过 session_list op 读取 SessionStore 数据由 Task 4 实现
#[tokio::test]
async fn session_list_reads_persisted_sessions_from_app_state_store() {
    use tempfile::TempDir;
    use ghostcode_router::session::SessionKey;

    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf());

    // 向 session_store 预存一条 session 数据
    // 使用 (group_id="grp1", actor_id="actor1", backend="claude") 作为 key
    let key: SessionKey = ("grp1".to_string(), "actor1".to_string(), "claude".to_string());
    state
        .session_store
        .save(key.clone(), "test-session-id-abc123".to_string())
        .expect("session_store.save 不应失败");

    // 通过 get() 验证持久化数据可以被读取回来
    let retrieved = state.session_store.get(&key);
    assert_eq!(
        retrieved.as_deref(),
        Some("test-session-id-abc123"),
        "session_store.get 应返回刚存入的 session_id"
    );

    // 通过 list() 验证 session 条目可被枚举
    let all_sessions = state.session_store.list();
    assert_eq!(
        all_sessions.len(),
        1,
        "session_store.list 应返回 1 条记录，实际: {:?}",
        all_sessions
    );

    // 验证 session 条目内容正确
    let (listed_key, listed_session_id) = &all_sessions[0];
    assert_eq!(listed_key, &key, "列出的 key 应与存入的 key 匹配");
    assert_eq!(
        listed_session_id,
        "test-session-id-abc123",
        "列出的 session_id 应与存入的值匹配"
    );

    // 同时验证通过 session_list op 调用时框架正常工作
    // （具体返回持久化数据由 Task 4 实现）
    let resp = dispatch(
        &state,
        make_req(
            "session_list",
            serde_json::json!({ "group_id": "grp1" }),
        ),
    )
    .await;
    assert!(resp.ok, "session_list op 框架应正常响应");
    assert!(
        resp.result["sessions"].is_array(),
        "sessions 字段应为数组类型"
    );
}
