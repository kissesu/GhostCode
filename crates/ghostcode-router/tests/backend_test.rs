/**
 * @file backend_test.rs
 * @description Backend trait 和三后端 CLI 参数构建器测试
 *              测试 CodexBackend、ClaudeBackend、GeminiBackend 的参数生成逻辑
 *              覆盖 new 模式和 resume 模式下的所有参数构建场景
 * @author Atlas.oi
 * @date 2026-03-02
 */

use ghostcode_router::backend::*;
use std::path::PathBuf;
use std::time::Duration;

// ============================================
// CodexBackend 测试组
// ============================================

/// 测试：Codex new 模式参数必须包含所需安全标志
#[test]
fn codex_new_args_contain_required_flags() {
    let backend = CodexBackend;
    let config = TaskConfig {
        workdir: PathBuf::from("/tmp/test"),
        mode: TaskMode::New,
        session_id: None,
        model: None,
        timeout: Duration::from_secs(3600),
    };
    let args = backend.build_args(&config);
    assert!(args.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
    assert!(args.contains(&"--skip-git-repo-check".to_string()));
    assert!(args.contains(&"--json".to_string()));
}

/// 测试：Codex new 模式参数不包含 -C 标志（工作目录改由 cmd.current_dir 设置）
/// 原因：Codex CLI 会将 -C 路径写入 websocket header，非 ASCII 路径会导致 UTF-8 编码错误
#[test]
fn codex_new_args_contain_workdir() {
    let backend = CodexBackend;
    let config = TaskConfig {
        workdir: PathBuf::from("/my/project"),
        mode: TaskMode::New,
        session_id: None,
        model: None,
        timeout: Duration::from_secs(3600),
    };
    let args = backend.build_args(&config);
    // -C 不应出现在参数中，工作目录由进程 cwd 提供
    assert!(!args.contains(&"-C".to_string()));
    // 必须包含 --json 输出标志
    assert!(args.contains(&"--json".to_string()));
}

/// 测试：Codex resume 模式不包含 -C 工作目录，但包含 resume 和 session_id
#[test]
fn codex_resume_no_workdir() {
    let backend = CodexBackend;
    let config = TaskConfig {
        workdir: PathBuf::from("/tmp/test"),
        mode: TaskMode::Resume,
        session_id: Some("sess123".to_string()),
        model: None,
        timeout: Duration::from_secs(3600),
    };
    let args = backend.build_args(&config);
    assert!(!args.contains(&"-C".to_string()));
    assert!(args.contains(&"resume".to_string()));
    assert!(args.contains(&"sess123".to_string()));
}

// ============================================
// ClaudeBackend 测试组
// ============================================

/// 测试：Claude new 模式参数必须包含 --setting-sources "" 和权限跳过标志
#[test]
fn claude_new_args_contain_setting_sources_empty() {
    let backend = ClaudeBackend;
    let config = TaskConfig {
        workdir: PathBuf::from("/tmp/test"),
        mode: TaskMode::New,
        session_id: None,
        model: None,
        timeout: Duration::from_secs(3600),
    };
    let args = backend.build_args(&config);
    assert!(args.contains(&"--setting-sources".to_string()));
    assert!(args.contains(&"".to_string()));
    assert!(args.contains(&"--dangerously-skip-permissions".to_string()));
}

/// 测试：Claude resume 模式必须包含 -r session_id 参数对
#[test]
fn claude_resume_contains_r_flag() {
    let backend = ClaudeBackend;
    let config = TaskConfig {
        workdir: PathBuf::from("/tmp/test"),
        mode: TaskMode::Resume,
        session_id: Some("claude-sess".to_string()),
        model: None,
        timeout: Duration::from_secs(3600),
    };
    let args = backend.build_args(&config);
    let r_idx = args.iter().position(|a| a == "-r").unwrap();
    assert_eq!(args[r_idx + 1], "claude-sess");
}

// ============================================
// GeminiBackend 测试组
// ============================================

/// 测试：Gemini new 模式参数必须包含 -m model 和 -y 标志
#[test]
fn gemini_new_args_contain_model() {
    let backend = GeminiBackend::new(Some("gemini-2.5-pro".to_string()));
    let config = TaskConfig {
        workdir: PathBuf::from("/tmp/test"),
        mode: TaskMode::New,
        session_id: None,
        model: Some("gemini-2.5-pro".to_string()),
        timeout: Duration::from_secs(3600),
    };
    let args = backend.build_args(&config);
    let m_idx = args.iter().position(|a| a == "-m").unwrap();
    assert_eq!(args[m_idx + 1], "gemini-2.5-pro");
    assert!(args.contains(&"-y".to_string()));
}

/// 测试：Gemini resume 模式必须包含 -r session_id 参数对
#[test]
fn gemini_resume_contains_r_flag() {
    let backend = GeminiBackend::new(Some("gemini-2.5-pro".to_string()));
    let config = TaskConfig {
        workdir: PathBuf::from("/tmp/test"),
        mode: TaskMode::Resume,
        session_id: Some("gem-sess".to_string()),
        model: Some("gemini-2.5-pro".to_string()),
        timeout: Duration::from_secs(3600),
    };
    let args = backend.build_args(&config);
    let r_idx = args.iter().position(|a| a == "-r").unwrap();
    assert_eq!(args[r_idx + 1], "gem-sess");
}

// ============================================
// Proptest 属性基测试
// ============================================

use proptest::prelude::*;

// 生成随机 TaskMode 的策略
fn arb_task_mode() -> impl Strategy<Value = TaskMode> {
    prop_oneof![Just(TaskMode::New), Just(TaskMode::Resume)]
}

// 生成随机 TaskConfig 的策略（session_id 随机非空字符串）
fn arb_task_config() -> impl Strategy<Value = TaskConfig> {
    (
        arb_task_mode(),
        proptest::option::of("[a-zA-Z0-9_-]{1,32}"),
        proptest::option::of("[a-zA-Z0-9_-]{1,32}"),
    )
        .prop_map(|(mode, session_id, model)| TaskConfig {
            workdir: PathBuf::from("/tmp/test"),
            mode,
            session_id,
            model,
            timeout: Duration::from_secs(3600),
        })
}

proptest! {
    /// 属性测试：任意 TaskConfig 下，Codex 后端参数不为空
    #[test]
    fn prop_codex_args_never_empty(config in arb_task_config()) {
        let backend = CodexBackend;
        let args = backend.build_args(&config);
        prop_assert!(!args.is_empty());
    }

    /// 属性测试：任意 TaskConfig 下，Claude 后端参数不为空
    #[test]
    fn prop_claude_args_never_empty(config in arb_task_config()) {
        let backend = ClaudeBackend;
        let args = backend.build_args(&config);
        prop_assert!(!args.is_empty());
    }

    /// 属性测试：任意 TaskConfig 下，Gemini 后端参数不为空
    #[test]
    fn prop_gemini_args_never_empty(config in arb_task_config()) {
        let backend = GeminiBackend::new(Some("gemini-2.5-pro".to_string()));
        let args = backend.build_args(&config);
        prop_assert!(!args.is_empty());
    }

    /// 属性测试：Resume 模式且有 session_id 时，Codex 参数包含 session_id
    #[test]
    fn prop_codex_resume_contains_session_id(session in "[a-zA-Z0-9_-]{1,32}") {
        let backend = CodexBackend;
        let config = TaskConfig {
            workdir: PathBuf::from("/tmp/test"),
            mode: TaskMode::Resume,
            session_id: Some(session.clone()),
            model: None,
            timeout: Duration::from_secs(3600),
        };
        let args = backend.build_args(&config);
        prop_assert!(args.contains(&session));
    }

    /// 属性测试：Resume 模式且有 session_id 时，Claude 参数包含 session_id
    #[test]
    fn prop_claude_resume_contains_session_id(session in "[a-zA-Z0-9_-]{1,32}") {
        let backend = ClaudeBackend;
        let config = TaskConfig {
            workdir: PathBuf::from("/tmp/test"),
            mode: TaskMode::Resume,
            session_id: Some(session.clone()),
            model: None,
            timeout: Duration::from_secs(3600),
        };
        let args = backend.build_args(&config);
        prop_assert!(args.contains(&session));
    }

    /// 属性测试：Resume 模式且有 session_id 时，Gemini 参数包含 session_id
    #[test]
    fn prop_gemini_resume_contains_session_id(session in "[a-zA-Z0-9_-]{1,32}") {
        let backend = GeminiBackend::new(Some("gemini-2.5-pro".to_string()));
        let config = TaskConfig {
            workdir: PathBuf::from("/tmp/test"),
            mode: TaskMode::Resume,
            session_id: Some(session.clone()),
            model: Some("gemini-2.5-pro".to_string()),
            timeout: Duration::from_secs(3600),
        };
        let args = backend.build_args(&config);
        prop_assert!(args.contains(&session));
    }
}
