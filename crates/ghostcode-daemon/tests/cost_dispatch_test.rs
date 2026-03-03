//! 成本分发器集成测试（T42）
//! 测试 cost_record/cost_summary 两个 op 的完整行为
//! @author Atlas.oi
//! @date 2026-03-03

use ghostcode_daemon::dispatch::dispatch;
use ghostcode_daemon::server::AppState;
use ghostcode_types::ipc::DaemonRequest;

// ============================================
// 测试辅助函数
// ============================================

/// 构造 cost_record 请求
fn make_record_req(
    group_id: &str,
    task_id: &str,
    model: &str,
    prompt_tokens: u64,
    completion_tokens: u64,
) -> DaemonRequest {
    DaemonRequest::new(
        "cost_record",
        serde_json::json!({
            "group_id": group_id,
            "task_id": task_id,
            "model": model,
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
        }),
    )
}

// ============================================
// 测试 1：cost_record 成功记录
// ============================================

/// 验证 cost_record 成功记录 usage，返回 ok: true
#[tokio::test]
async fn cost_record_stores_usage() {
    let state = AppState::default();
    let req = make_record_req("g1", "t1", "claude-opus", 100, 50);
    let resp = dispatch(&state, req).await;

    // 应成功返回 ok: true
    assert!(resp.ok, "cost_record 应返回 ok: true，实际: {:?}", resp.error);
    assert!(resp.error.is_none(), "cost_record 不应有 error");
    // 结果中应包含 recorded 字段
    assert_eq!(
        resp.result["recorded"],
        serde_json::json!(true),
        "cost_record 结果应包含 recorded: true"
    );
}

// ============================================
// 测试 2：cost_summary 聚合查询
// ============================================

/// 验证先记录两条 usage 后，cost_summary 返回正确聚合数据
#[tokio::test]
async fn cost_summary_returns_aggregated_result() {
    let state = AppState::default();

    // 记录两条 usage
    let req1 = make_record_req("g1", "t1", "claude-opus", 100, 50);
    let resp1 = dispatch(&state, req1).await;
    assert!(resp1.ok, "第一条 cost_record 应成功");

    let req2 = make_record_req("g1", "t1", "claude-opus", 200, 100);
    let resp2 = dispatch(&state, req2).await;
    assert!(resp2.ok, "第二条 cost_record 应成功");

    // 查询聚合
    let summary_req = DaemonRequest::new(
        "cost_summary",
        serde_json::json!({ "group_id": "g1" }),
    );
    let resp = dispatch(&state, summary_req).await;

    assert!(resp.ok, "cost_summary 应返回 ok: true");
    assert!(resp.error.is_none(), "cost_summary 不应有 error");

    // 验证聚合数据字段存在
    let result = &resp.result;
    assert!(
        result["total_prompt_tokens"].is_number(),
        "应包含 total_prompt_tokens 字段"
    );
    assert!(
        result["total_completion_tokens"].is_number(),
        "应包含 total_completion_tokens 字段"
    );
    assert!(
        result["total_cost_micro"].is_number(),
        "应包含 total_cost_micro 字段"
    );
    assert!(
        result["request_count"].is_number(),
        "应包含 request_count 字段"
    );

    // 验证累加数值：100+200=300 prompt tokens，50+100=150 completion tokens
    assert_eq!(result["total_prompt_tokens"], serde_json::json!(300u64));
    assert_eq!(result["total_completion_tokens"], serde_json::json!(150u64));
    assert_eq!(result["request_count"], serde_json::json!(2u64));
    // total_cost_micro 应大于 0（claude-opus 有价格）
    assert!(
        result["total_cost_micro"].as_u64().unwrap_or(0) > 0,
        "claude-opus 的成本应大于 0"
    );
}

// ============================================
// 测试 3：缺少 group_id 返回 INVALID_ARGS
// ============================================

/// 验证 cost_record 缺少 group_id 时返回 INVALID_ARGS
#[tokio::test]
async fn cost_record_missing_group_id() {
    let state = AppState::default();
    let req = DaemonRequest::new(
        "cost_record",
        serde_json::json!({
            "task_id": "t1",
            "model": "claude-opus",
            "prompt_tokens": 100,
            "completion_tokens": 50,
        }),
    );
    let resp = dispatch(&state, req).await;

    assert!(!resp.ok, "缺少 group_id 应返回 ok: false");
    assert!(resp.error.is_some(), "应包含 error 信息");
    assert_eq!(
        resp.error.as_ref().unwrap().code,
        "INVALID_ARGS",
        "错误码应为 INVALID_ARGS"
    );
}

// ============================================
// 测试 4：缺少 model 返回 INVALID_ARGS
// ============================================

/// 验证 cost_record 缺少 model 时返回 INVALID_ARGS
#[tokio::test]
async fn cost_record_missing_model() {
    let state = AppState::default();
    let req = DaemonRequest::new(
        "cost_record",
        serde_json::json!({
            "group_id": "g1",
            "task_id": "t1",
            "prompt_tokens": 100,
            "completion_tokens": 50,
        }),
    );
    let resp = dispatch(&state, req).await;

    assert!(!resp.ok, "缺少 model 应返回 ok: false");
    assert!(resp.error.is_some(), "应包含 error 信息");
    assert_eq!(
        resp.error.as_ref().unwrap().code,
        "INVALID_ARGS",
        "错误码应为 INVALID_ARGS"
    );
}

// ============================================
// 测试 5：未记录任何 usage 时 cost_summary 返回零值
// ============================================

/// 验证空状态下 cost_summary 返回零值快照
#[tokio::test]
async fn cost_summary_empty_returns_zero() {
    let state = AppState::default();
    let req = DaemonRequest::new(
        "cost_summary",
        serde_json::json!({ "group_id": "empty-group" }),
    );
    let resp = dispatch(&state, req).await;

    assert!(resp.ok, "cost_summary 应返回 ok: true");
    let result = &resp.result;
    assert_eq!(result["total_prompt_tokens"], serde_json::json!(0u64));
    assert_eq!(result["total_completion_tokens"], serde_json::json!(0u64));
    assert_eq!(result["total_cost_micro"], serde_json::json!(0u64));
    assert_eq!(result["request_count"], serde_json::json!(0u64));
}

// ============================================
// 测试 6：未知模型标记为 estimated，summary 仍正常
// ============================================

/// 验证记录未知模型后，summary 仍正常返回（source 自动标记为 estimated）
#[tokio::test]
async fn cost_record_unknown_model_marks_estimated() {
    let state = AppState::default();
    // 使用未知模型名
    let req = make_record_req("g-unknown", "t1", "unknown-model-xyz", 100, 50);
    let resp = dispatch(&state, req).await;

    // 即使是未知模型也应成功记录
    assert!(resp.ok, "未知模型也应成功记录，返回 ok: true");

    // 查询 summary，应正常返回（使用默认价格估算成本）
    let summary_req = DaemonRequest::new(
        "cost_summary",
        serde_json::json!({ "group_id": "g-unknown" }),
    );
    let summary_resp = dispatch(&state, summary_req).await;

    assert!(summary_resp.ok, "未知模型的 summary 应返回 ok: true");
    assert_eq!(summary_resp.result["request_count"], serde_json::json!(1u64));
    // 未知模型使用默认价格估算，成本应大于 0
    assert!(
        summary_resp.result["total_cost_micro"].as_u64().unwrap_or(0) > 0,
        "未知模型使用默认价格估算，成本应大于 0"
    );
}

// ============================================
// 测试 7：非法 source 值返回 INVALID_ARGS
// ============================================

/// 验证 cost_record 传入未知 source 值时返回 INVALID_ARGS
#[tokio::test]
async fn cost_record_invalid_source_returns_error() {
    let state = AppState::default();
    let req = DaemonRequest::new(
        "cost_record",
        serde_json::json!({
            "group_id": "g1",
            "model": "claude-opus",
            "source": "typo_vendor",
        }),
    );
    let resp = dispatch(&state, req).await;

    assert!(!resp.ok, "非法 source 应返回 ok: false");
    assert_eq!(
        resp.error.as_ref().unwrap().code,
        "INVALID_ARGS",
        "非法 source 错误码应为 INVALID_ARGS"
    );
    assert!(
        resp.error.as_ref().unwrap().message.contains("typo_vendor"),
        "错误消息应包含非法 source 值"
    );
}

// ============================================
// 测试 8：prompt_tokens 超 u32::MAX 返回 INVALID_ARGS
// ============================================

/// 验证 cost_record 的 prompt_tokens 超出 u32 范围时返回 INVALID_ARGS
#[tokio::test]
async fn cost_record_overflow_prompt_tokens() {
    let state = AppState::default();
    let overflow_value = u32::MAX as u64 + 1;
    let req = DaemonRequest::new(
        "cost_record",
        serde_json::json!({
            "group_id": "g1",
            "model": "claude-opus",
            "prompt_tokens": overflow_value,
            "completion_tokens": 0,
        }),
    );
    let resp = dispatch(&state, req).await;

    assert!(!resp.ok, "超出 u32 范围的 prompt_tokens 应返回 ok: false");
    assert_eq!(
        resp.error.as_ref().unwrap().code,
        "INVALID_ARGS",
        "超出范围错误码应为 INVALID_ARGS"
    );
}

// ============================================
// 测试 9：completion_tokens 超 u32::MAX 返回 INVALID_ARGS
// ============================================

/// 验证 cost_record 的 completion_tokens 超出 u32 范围时返回 INVALID_ARGS
#[tokio::test]
async fn cost_record_overflow_completion_tokens() {
    let state = AppState::default();
    let overflow_value = u32::MAX as u64 + 1;
    let req = DaemonRequest::new(
        "cost_record",
        serde_json::json!({
            "group_id": "g1",
            "model": "claude-opus",
            "prompt_tokens": 0,
            "completion_tokens": overflow_value,
        }),
    );
    let resp = dispatch(&state, req).await;

    assert!(!resp.ok, "超出 u32 范围的 completion_tokens 应返回 ok: false");
    assert_eq!(
        resp.error.as_ref().unwrap().code,
        "INVALID_ARGS",
        "超出范围错误码应为 INVALID_ARGS"
    );
}

// ============================================
// 测试 10：按 task_id 过滤查询
// ============================================

/// 验证 cost_summary 支持按 task_id 过滤
#[tokio::test]
async fn cost_summary_filter_by_task() {
    let state = AppState::default();

    // 记录 task-A 的两条 usage
    let req_a1 = make_record_req("g-filter", "task-A", "claude-opus", 100, 50);
    dispatch(&state, req_a1).await;
    let req_a2 = make_record_req("g-filter", "task-A", "claude-sonnet", 200, 80);
    dispatch(&state, req_a2).await;

    // 记录 task-B 的一条 usage
    let req_b = make_record_req("g-filter", "task-B", "claude-opus", 500, 200);
    dispatch(&state, req_b).await;

    // 仅查询 task-A
    let filter_req = DaemonRequest::new(
        "cost_summary",
        serde_json::json!({
            "group_id": "g-filter",
            "task_id": "task-A",
        }),
    );
    let resp = dispatch(&state, filter_req).await;

    assert!(resp.ok, "按 task_id 过滤的 cost_summary 应返回 ok: true");
    // task-A 共 2 条记录：100+200=300 prompt tokens，50+80=130 completion tokens
    assert_eq!(resp.result["total_prompt_tokens"], serde_json::json!(300u64));
    assert_eq!(resp.result["total_completion_tokens"], serde_json::json!(130u64));
    assert_eq!(resp.result["request_count"], serde_json::json!(2u64));
}
