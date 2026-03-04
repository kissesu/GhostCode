//! @file sovereignty_enforcement_test.rs
//! @description SovereigntyGuard 执行期写入约束测试
//!              验证 enforce_execution() 在进程 spawn 前正确拦截非 Claude 后端的写入操作。
//!              测试覆盖：确定性用例 + 属性基测试 (PBT)
//! @author Atlas.oi
//! @date 2026-03-04

use ghostcode_router::sovereignty::{classify_write_intent, enforce_execution, WriteIntent};

// ============================================================
// 用例 1：codex 后端的写入命令在 spawn 前被拒绝
// ============================================================

#[test]
fn codex_write_command_blocked_before_spawn() {
    // 写入型命令：write_file, edit_file, create_file, delete_file 等
    let write_commands = [
        ("write_file", vec!["path/to/file.rs".to_string(), "content".to_string()]),
        ("edit_file", vec!["path/to/file.rs".to_string()]),
        ("create_file", vec!["new_file.txt".to_string()]),
        ("delete_file", vec!["old_file.txt".to_string()]),
        ("remove_dir", vec!["dir/".to_string()]),
        ("modify_config", vec!["config.toml".to_string()]),
        ("update_schema", vec![]),
        ("patch_file", vec!["fix.patch".to_string()]),
    ];

    for (cmd, args) in &write_commands {
        let result = enforce_execution("codex", cmd, args);
        assert!(
            result.is_err(),
            "codex 后端写入命令 '{}' 应该被拒绝，但返回了 Ok",
            cmd
        );
    }
}

// ============================================================
// 用例 2：claude 后端的写入命令通过 preflight 检查
// ============================================================

#[test]
fn claude_write_command_passes_preflight() {
    // 与用例 1 相同的写入命令，但后端为 claude
    let write_commands = [
        ("write_file", vec!["path/to/file.rs".to_string(), "content".to_string()]),
        ("edit_file", vec!["path/to/file.rs".to_string()]),
        ("create_file", vec!["new_file.txt".to_string()]),
        ("delete_file", vec!["old_file.txt".to_string()]),
    ];

    for (cmd, args) in &write_commands {
        let result = enforce_execution("claude", cmd, args);
        assert!(
            result.is_ok(),
            "claude 后端写入命令 '{}' 应该通过，但返回了 Err: {:?}",
            cmd,
            result.err()
        );
    }
}

// ============================================================
// 用例 3：所有后端的只读命令通过检查
// ============================================================

#[test]
fn read_commands_pass_for_all_backends() {
    // 只读命令：不涉及写入操作
    let read_commands = [
        ("read_file", vec!["path/to/file.rs".to_string()]),
        ("list_dir", vec!["./src".to_string()]),
        ("search", vec!["pattern".to_string()]),
        ("find_file", vec!["*.rs".to_string()]),
        ("get_symbol", vec!["MyStruct".to_string()]),
        ("show_diff", vec![]),
        ("cat", vec!["file.txt".to_string()]),
        ("ls", vec!["-la".to_string()]),
        ("grep", vec!["pattern".to_string(), "file.rs".to_string()]),
    ];

    let backends = ["claude", "codex", "gemini", "gpt4"];

    for backend in &backends {
        for (cmd, args) in &read_commands {
            let result = enforce_execution(backend, cmd, args);
            assert!(
                result.is_ok(),
                "后端 '{}' 的只读命令 '{}' 应该通过，但返回了 Err: {:?}",
                backend,
                cmd,
                result.err()
            );
        }
    }
}

// ============================================================
// 用例 4：未知命令对非 claude 后端默认拒绝（安全优先）
// ============================================================

#[test]
fn unknown_command_defaults_to_blocked_for_non_claude() {
    // 无法分类的命令名（不包含已知读写关键词）
    let unknown_commands = [
        ("xyzzy_op", vec![]),
        ("do_thing", vec!["arg1".to_string()]),
        ("process_data", vec![]),  // "process" 无法判断读写
        ("run_task", vec![]),
    ];

    let non_claude_backends = ["codex", "gemini", "gpt4", "ollama"];

    for backend in &non_claude_backends {
        for (cmd, args) in &unknown_commands {
            let result = enforce_execution(backend, cmd, args);
            assert!(
                result.is_err(),
                "非 claude 后端 '{}' 的未知命令 '{}' 应该默认拒绝，但返回了 Ok",
                backend,
                cmd
            );
        }
    }

    // claude 后端的未知命令应该通过（因为 claude 是写入者）
    for (cmd, args) in &unknown_commands {
        let result = enforce_execution("claude", cmd, args);
        assert!(
            result.is_ok(),
            "claude 后端的未知命令 '{}' 应该通过（claude 为可信后端），但返回了 Err",
            cmd
        );
    }
}

// ============================================================
// 用例 5 (PBT)：随机写入命令对非 claude 后端始终被拒绝
// 使用 proptest 生成随机命令，验证包含写入关键词时始终拒绝
// ============================================================

#[cfg(test)]
mod pbt_tests {
    use super::*;
    use proptest::prelude::*;

    // 写入关键词列表：这些词出现在命令名中，表示写入意图
    const WRITE_KEYWORDS: &[&str] = &[
        "write", "edit", "create", "delete", "remove", "modify",
        "update", "patch", "mv", "cp", "chmod",
    ];

    proptest! {
        /// 随机写入命令对非 claude 后端始终被拒绝
        ///
        /// 策略：从写入关键词列表中随机选择一个，构造命令名，
        /// 验证非 claude 后端对所有包含写入关键词的命令都拒绝
        #[test]
        fn random_write_commands_never_bypass_for_non_claude(
            // 随机选择写入关键词索引
            keyword_idx in 0usize..11usize,
            // 随机命令后缀（避免固定前缀被特殊处理）
            suffix in "[a-z0-9_]{0,10}",
            // 随机非 claude 后端名
            backend in prop::sample::select(vec!["codex", "gemini", "gpt4", "ollama", "anthropic-other"]),
        ) {
            let keyword = WRITE_KEYWORDS[keyword_idx];
            // 构造以写入关键词开头的命令名
            let command = format!("{}_{}", keyword, suffix);

            let result = enforce_execution(backend, &command, &[]);
            prop_assert!(
                result.is_err(),
                "非 claude 后端 '{}' 的写入命令 '{}' 应该始终被拒绝",
                backend,
                command
            );
        }
    }
}

// ============================================================
// classify_write_intent 单元测试
// ============================================================

#[test]
fn classify_write_intent_identifies_write_operations() {
    // 明确的写入命令
    assert_eq!(
        classify_write_intent("write_file", &[]),
        WriteIntent::Write
    );
    assert_eq!(
        classify_write_intent("edit_code", &[]),
        WriteIntent::Write
    );
    assert_eq!(
        classify_write_intent("create_dir", &[]),
        WriteIntent::Write
    );
}

#[test]
fn classify_write_intent_identifies_read_operations() {
    // 明确的只读命令
    assert_eq!(
        classify_write_intent("read_file", &[]),
        WriteIntent::ReadOnly
    );
    assert_eq!(
        classify_write_intent("list_dir", &[]),
        WriteIntent::ReadOnly
    );
    assert_eq!(
        classify_write_intent("search_code", &[]),
        WriteIntent::ReadOnly
    );
    assert_eq!(
        classify_write_intent("find_symbol", &[]),
        WriteIntent::ReadOnly
    );
    assert_eq!(
        classify_write_intent("get_config", &[]),
        WriteIntent::ReadOnly
    );
}

#[test]
fn classify_write_intent_returns_unknown_for_ambiguous() {
    // 无法分类的命令名
    assert_eq!(
        classify_write_intent("xyzzy_op", &[]),
        WriteIntent::Unknown
    );
    assert_eq!(
        classify_write_intent("run_task", &[]),
        WriteIntent::Unknown
    );
}
