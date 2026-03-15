//! Session Gate 测试套件
//!
//! 验证 SessionGateStore 的核心行为：
//! - open/submit/abort 基本流程
//! - submit 自动关闭机制（SubmitResult::Complete）
//! - Bypass 协议（额度用完时的出口）
//! - State file 生命周期管理
//! - close() 手动关闭（可选，向后兼容）
//!
//! @author Atlas.oi
//! @date 2026-03-07

use std::path::PathBuf;

use ghostcode_daemon::session_gate::{SessionGateStore, SessionGateError, SubmitResult};
use tempfile::TempDir;

// ============================================
// 辅助函数
// ============================================

/// 创建测试环境：TempDir + SessionGateStore
///
/// 业务逻辑：
/// 1. 创建临时目录用于 state file 存储
/// 2. 构造 SessionGateStore 实例
///
/// @return (TempDir 保持所有权防止提前 drop, SessionGateStore)
fn setup() -> (TempDir, SessionGateStore) {
    let dir = TempDir::new().expect("创建临时目录失败");
    let state_file_path: PathBuf = dir.path().join("session_gate_state.json");
    let store = SessionGateStore::new(state_file_path);
    (dir, store)
}

// ============================================
// 测试用例
// ============================================

/// 测试 1：open 返回唯一的 session ID
///
/// 验证：
/// - 两次 open 返回不同的 UUID
/// - session_id 非空
#[test]
fn test_open_returns_unique_session_id() {
    let (_dir, store) = setup();

    // 第一次 open：创建 review 任务，需要 codex 和 gemini 两个模型
    let session_id_1 = store.open("review", vec!["codex", "gemini"]).expect("open 失败");
    // 第二次 open：创建另一个 review 任务
    let session_id_2 = store.open("review", vec!["codex", "gemini"]).expect("open 失败");

    // 两次 open 应返回不同的 session ID
    assert_ne!(session_id_1, session_id_2, "两次 open 应返回不同的 session ID");
    // session ID 不应为空
    assert!(!session_id_1.is_empty(), "session ID 不应为空");
    assert!(!session_id_2.is_empty(), "session ID 不应为空");
}

/// 测试 2：submit 单个模型后返回 Pending
///
/// 验证：
/// - submit codex 输出后返回 SubmitResult::Pending
/// - session 仍然打开（还有 gemini 未提交）
#[test]
fn test_submit_single_model_returns_pending() {
    let (_dir, store) = setup();

    // 打开一个 review session，需要 codex 和 gemini
    let session_id = store.open("review", vec!["codex", "gemini"]).expect("open 失败");

    // codex 提交审查结果
    let result = store.submit(
        &session_id,
        "codex",
        "findings",
        serde_json::json!({"issues": ["unused import"]}),
        false,
        None,
    ).expect("submit codex 应成功");

    // 只有 codex 提交，gemini 尚未提交 → 返回 Pending
    assert!(
        matches!(result, SubmitResult::Pending),
        "单个模型提交后应返回 Pending，实际为: {:?}",
        result
    );
}

/// 测试 3：所有模型都提交后 submit 自动关闭并返回 Complete
///
/// 验证：
/// - codex 提交 → Pending
/// - gemini 提交 → Complete(CombinedOutput)（自动关闭）
/// - CombinedOutput.partial == false（完整 session）
/// - CombinedOutput.submissions 包含两个模型的提交
#[test]
fn test_submit_auto_closes_when_all_models_submitted() {
    let (_dir, store) = setup();

    let session_id = store.open("review", vec!["codex", "gemini"]).expect("open 失败");

    // codex 提交 → 应返回 Pending
    let result1 = store.submit(
        &session_id,
        "codex",
        "findings",
        serde_json::json!({"issues": ["unused import"]}),
        false,
        None,
    ).expect("codex submit 失败");
    assert!(matches!(result1, SubmitResult::Pending), "第一个模型提交应返回 Pending");

    // gemini 提交 → 应返回 Complete（自动关闭）
    let result2 = store.submit(
        &session_id,
        "gemini",
        "findings",
        serde_json::json!({"issues": ["missing doc"]}),
        false,
        None,
    ).expect("gemini submit 失败");

    match result2 {
        SubmitResult::Complete(output) => {
            // 完整 session：partial 应为 false
            assert!(!output.partial, "两个模型都提交后 partial 应为 false");
            // submissions 应包含 2 个条目
            assert_eq!(output.submissions.len(), 2, "应包含 codex 和 gemini 的提交");
            // missing_models 应为空
            assert!(output.missing_models.is_empty(), "missing_models 应为空");
        }
        SubmitResult::Pending => panic!("所有模型提交后应返回 Complete，而非 Pending"),
    }
}

/// 测试 4：只有 codex 提交时手动 close 失败
///
/// 验证：
/// - 只提交 codex，未提交 gemini → close 返回 SESSION_INCOMPLETE 错误
/// - 错误中应包含 missing_models 信息，包含 "gemini"
#[test]
fn test_close_fails_when_only_codex() {
    let (_dir, store) = setup();

    let session_id = store.open("review", vec!["codex", "gemini"]).expect("open 失败");

    // 只有 codex 提交（返回 Pending）
    let result = store.submit(
        &session_id,
        "codex",
        "findings",
        serde_json::json!({"issues": []}),
        false,
        None,
    ).expect("codex submit 失败");
    assert!(matches!(result, SubmitResult::Pending));

    // 手动 close 应该失败：gemini 尚未提交
    let close_result = store.close(&session_id);
    assert!(close_result.is_err(), "只有 codex 提交时 close 应失败");

    // 验证错误类型和缺失模型信息
    match close_result.unwrap_err() {
        SessionGateError::SessionIncomplete { missing_models } => {
            assert!(
                missing_models.contains(&"gemini".to_string()),
                "missing_models 应包含 gemini，实际为: {:?}",
                missing_models
            );
        }
        other => panic!("期望 SessionIncomplete 错误，实际为: {:?}", other),
    }
}

/// 测试 5：未提交任何模型时 close 失败
///
/// 验证：
/// - 未提交任何模型 → close 返回 SESSION_INCOMPLETE 错误
/// - missing_models 应包含所有要求的模型
#[test]
fn test_close_fails_when_neither_submitted() {
    let (_dir, store) = setup();

    let session_id = store.open("review", vec!["codex", "gemini"]).expect("open 失败");

    // 不做任何提交，直接 close
    let result = store.close(&session_id);
    assert!(result.is_err(), "未提交任何模型时 close 应失败");

    match result.unwrap_err() {
        SessionGateError::SessionIncomplete { missing_models } => {
            // 两个模型都应在 missing 列表中
            assert!(
                missing_models.contains(&"codex".to_string()),
                "missing_models 应包含 codex"
            );
            assert!(
                missing_models.contains(&"gemini".to_string()),
                "missing_models 应包含 gemini"
            );
        }
        other => panic!("期望 SessionIncomplete 错误，实际为: {:?}", other),
    }
}

/// 测试 6：向不存在的 session 提交返回 NotFound 错误
///
/// 验证：
/// - 使用无效的 session_id 进行 submit → SessionGateError::NotFound
#[test]
fn test_submit_to_unknown_session_returns_error() {
    let (_dir, store) = setup();

    // 使用一个不存在的 session ID
    let fake_session_id = "nonexistent-session-id-12345";

    let result = store.submit(
        fake_session_id,
        "codex",
        "findings",
        serde_json::json!({}),
        false,
        None,
    );

    assert!(result.is_err(), "向不存在的 session 提交应返回错误");
    match result.unwrap_err() {
        SessionGateError::NotFound => {}
        other => panic!("期望 NotFound 错误，实际为: {:?}", other),
    }
}

/// 测试 7：bypass 提交自动关闭，且 partial=true
///
/// 验证：
/// - bypass=true 的提交触发自动关闭（即使 gemini 未提交）
/// - 返回 SubmitResult::Complete(CombinedOutput)
/// - CombinedOutput.partial == true
#[test]
fn test_bypass_submit_auto_closes() {
    let (_dir, store) = setup();

    let session_id = store.open("review", vec!["codex", "gemini"]).expect("open 失败");

    // codex 以 bypass=true 方式提交（表示额度用完等特殊情况）
    let result = store.submit(
        &session_id,
        "codex",
        "findings",
        serde_json::json!({"reason": "quota_exceeded"}),
        true,  // bypass = true
        None,
    ).expect("bypass submit 失败");

    // bypass 提交应触发自动关闭
    match result {
        SubmitResult::Complete(output) => {
            // partial 应为 true，因为不是所有模型都正常提交
            assert!(output.partial, "bypass close 的 partial 应为 true");
            // gemini 应在 missing_models 中
            assert!(
                output.missing_models.contains(&"gemini".to_string()),
                "missing_models 应包含 gemini"
            );
        }
        SubmitResult::Pending => panic!("bypass 提交应触发自动关闭，而非返回 Pending"),
    }
}

/// 测试 8：abort 清理 session，再次操作返回 NotFound
///
/// 验证：
/// - abort 成功后，再次 close 该 session 返回 NotFound
/// - session 被彻底清理
#[test]
fn test_abort_cleans_up_session() {
    let (_dir, store) = setup();

    let session_id = store.open("review", vec!["codex", "gemini"]).expect("open 失败");

    // 终止 session
    store.abort(&session_id).expect("abort 应成功");

    // abort 后再 close 应返回 NotFound
    let result = store.close(&session_id);
    assert!(result.is_err(), "abort 后 close 应失败");
    match result.unwrap_err() {
        SessionGateError::NotFound => {}
        other => panic!("期望 NotFound 错误，实际为: {:?}", other),
    }
}

/// 测试 9：open 后 state file 应存在
///
/// 验证：
/// - open 创建 session 后，对应的 state file 应写入磁盘
/// - state file 路径与构造时指定的一致
#[test]
fn test_state_file_created_on_open() {
    let dir = TempDir::new().expect("创建临时目录失败");
    let state_file_path: PathBuf = dir.path().join("session_gate_state.json");
    let store = SessionGateStore::new(state_file_path.clone());

    // open 前 state file 不应存在
    assert!(!state_file_path.exists(), "open 前 state file 不应存在");

    // open 一个 session
    let _session_id = store.open("review", vec!["codex", "gemini"]).expect("open 失败");

    // open 后 state file 应存在
    assert!(state_file_path.exists(), "open 后 state file 应存在");
}

/// 测试 10：submit 自动关闭 / abort 后 state file 应被删除
///
/// 验证：
/// - submit 自动关闭后 state file 被清理（当无活跃 session 时）
/// - abort 后 state file 也被清理（当无活跃 session 时）
#[test]
fn test_state_file_deleted_on_auto_close_or_abort() {
    // ---- 测试 submit 自动关闭清理 ----
    let dir = TempDir::new().expect("创建临时目录失败");
    let state_file_path: PathBuf = dir.path().join("session_gate_state.json");
    let store = SessionGateStore::new(state_file_path.clone());

    let session_id = store.open("review", vec!["codex", "gemini"]).expect("open 失败");
    assert!(state_file_path.exists(), "open 后 state file 应存在");

    // 提交所有模型（第二次 submit 触发自动关闭）
    store.submit(&session_id, "codex", "findings", serde_json::json!({}), false, None)
        .expect("codex submit 失败");
    let result = store.submit(&session_id, "gemini", "findings", serde_json::json!({}), false, None)
        .expect("gemini submit 失败");

    // 验证自动关闭已触发
    assert!(matches!(result, SubmitResult::Complete(_)), "第二次 submit 应触发自动关闭");

    // 自动关闭后 state file 应被删除（无活跃 session）
    assert!(!state_file_path.exists(), "自动关闭后 state file 应被删除");

    // ---- 测试 abort 清理 ----
    let dir2 = TempDir::new().expect("创建临时目录失败");
    let state_file_path2: PathBuf = dir2.path().join("session_gate_state.json");
    let store2 = SessionGateStore::new(state_file_path2.clone());

    let session_id2 = store2.open("review", vec!["codex"]).expect("open 失败");
    assert!(state_file_path2.exists(), "open 后 state file 应存在");

    // abort
    store2.abort(&session_id2).expect("abort 失败");

    // abort 后 state file 应被删除（无活跃 session）
    assert!(!state_file_path2.exists(), "abort 后 state file 应被删除");
}

/// 测试 11：自动关闭后再提交返回 NotFound
///
/// 验证：
/// - session 自动关闭后，再次 submit 返回 NotFound
/// - session 被彻底清理，不可再使用
#[test]
fn test_submit_after_auto_close_returns_not_found() {
    let (_dir, store) = setup();

    let session_id = store.open("review", vec!["codex", "gemini"]).expect("open 失败");

    // codex 提交 → Pending
    store.submit(&session_id, "codex", "findings", serde_json::json!({}), false, None)
        .expect("codex submit 失败");

    // gemini 提交 → Complete（自动关闭）
    let result = store.submit(&session_id, "gemini", "findings", serde_json::json!({}), false, None)
        .expect("gemini submit 失败");
    assert!(matches!(result, SubmitResult::Complete(_)));

    // 自动关闭后再提交应返回 NotFound
    let err_result = store.submit(&session_id, "codex", "findings", serde_json::json!({}), false, None);
    assert!(err_result.is_err(), "自动关闭后再提交应返回错误");
    match err_result.unwrap_err() {
        SessionGateError::NotFound => {}
        other => panic!("期望 NotFound 错误，实际为: {:?}", other),
    }
}
