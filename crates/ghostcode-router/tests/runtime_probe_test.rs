// @file runtime_probe_test.rs
// @description ghostcode-router runtime_probe 模块的契约测试
//              验证 CLI 工具探测逻辑的正确性：
//              1. 可用命令应返回 Available + 版本信息
//              2. 不存在的命令应返回 Unavailable（不 panic）
//              3. probe_all() 应返回包含三个后端状态的 Vec
//              4. PBT: 任意命令名调用不导致 panic
// @author Atlas.oi
// @date 2026-03-04

use ghostcode_router::runtime_probe::{
    probe_all, probe_claude, probe_codex, probe_gemini, probe_runtime, RuntimeAvailability,
};
use proptest::prelude::*;

// ============================================
// 基础功能测试：已知存在的命令
// ============================================

/// 测试 probe_runtime 对一定存在的命令（echo）返回 Available
///
/// 注意：echo 在所有 Unix 系统上都存在，且支持 --version（部分系统）或直接执行
/// 使用 "ls" 代替 echo，因为 echo 是 shell 内建命令，部分系统可能找不到可执行文件
#[test]
fn test_probe_existing_command_returns_available() {
    // 使用 "ls" 命令（Unix 系统必有）
    let status = probe_runtime("ls");
    assert_eq!(status.command, "ls");
    // ls 应当可用（存在于 PATH 中）
    match &status.availability {
        RuntimeAvailability::Available { version } => {
            // 版本信息可以为空字符串，但不能是错误信息
            assert!(!version.contains("错误"), "版本信息不应包含错误: {}", version);
        }
        RuntimeAvailability::Unavailable { reason } => {
            panic!("ls 命令应该可用，但返回 Unavailable: {}", reason);
        }
    }
}

// ============================================
// 基础功能测试：不存在的命令
// ============================================

/// 测试 probe_runtime 对不存在的命令返回 Unavailable，不 panic
#[test]
fn test_probe_nonexistent_command_returns_unavailable() {
    let status = probe_runtime("nonexistent_command_xyz_12345");
    assert_eq!(status.command, "nonexistent_command_xyz_12345");
    match &status.availability {
        RuntimeAvailability::Unavailable { reason } => {
            // 原因信息不能为空
            assert!(!reason.is_empty(), "Unavailable 原因不应为空");
        }
        RuntimeAvailability::Available { .. } => {
            panic!("不存在的命令不应返回 Available");
        }
    }
}

// ============================================
// RuntimeStatus 结构测试
// ============================================

/// 测试 RuntimeStatus 结构体包含 name, command, availability 字段
#[test]
fn test_runtime_status_has_required_fields() {
    let status = probe_runtime("ls");
    // name 字段：probe_runtime 使用 command 本身作为 name
    assert!(!status.name.is_empty(), "name 字段不应为空");
    // command 字段：应等于传入的命令名
    assert_eq!(status.command, "ls");
    // availability 字段：应为 RuntimeAvailability 枚举的某个变体
    // （编译通过即验证存在该字段）
    let _ = &status.availability;
}

// ============================================
// probe_all 测试
// ============================================

/// 测试 probe_all() 返回包含三个元素的 Vec<RuntimeStatus>
#[test]
fn test_probe_all_returns_three_results() {
    let results = probe_all();
    assert_eq!(results.len(), 3, "probe_all 应返回 3 个结果（Codex/Claude/Gemini）");
    // 验证每个元素的 name 字段
    let names: Vec<&str> = results.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Codex"), "结果应包含 Codex");
    assert!(names.contains(&"Claude"), "结果应包含 Claude");
    assert!(names.contains(&"Gemini"), "结果应包含 Gemini");
}

/// 测试 probe_all() 中的每个元素都有 command 字段
#[test]
fn test_probe_all_elements_have_command() {
    let results = probe_all();
    for status in &results {
        assert!(!status.command.is_empty(), "每个 RuntimeStatus 的 command 不应为空");
    }
}

// ============================================
// 辅助函数测试：probe_codex, probe_claude, probe_gemini
// ============================================

/// 测试 probe_codex() 存在且可调用，返回 name = "Codex"
#[test]
fn test_probe_codex_exists_and_callable() {
    let status = probe_codex();
    assert_eq!(status.name, "Codex", "probe_codex 的 name 应为 Codex");
    assert_eq!(status.command, "codex", "probe_codex 的 command 应为 codex");
    // 只验证可以调用，不断言具体 availability（本机可能没有 codex）
}

/// 测试 probe_claude() 存在且可调用，返回 name = "Claude"
#[test]
fn test_probe_claude_exists_and_callable() {
    let status = probe_claude();
    assert_eq!(status.name, "Claude", "probe_claude 的 name 应为 Claude");
    assert_eq!(status.command, "claude", "probe_claude 的 command 应为 claude");
}

/// 测试 probe_gemini() 存在且可调用，返回 name = "Gemini"
#[test]
fn test_probe_gemini_exists_and_callable() {
    let status = probe_gemini();
    assert_eq!(status.name, "Gemini", "probe_gemini 的 name 应为 Gemini");
    assert_eq!(status.command, "gemini", "probe_gemini 的 command 应为 gemini");
}

// ============================================
// PBT（基于属性的测试）：任意命令名不导致 panic
// ============================================

// 属性测试：对任意随机字符串命令名调用 probe_runtime，不 panic
// 返回值只能是 Available 或 Unavailable，两种情况都合法
proptest! {
    #[test]
    fn pbt_probe_runtime_never_panics(cmd in "[a-zA-Z0-9_\\-]{1,50}") {
        // 无论输入什么命令名，probe_runtime 都不应 panic
        let status = probe_runtime(&cmd);
        // 验证返回值是合法枚举变体（不 panic 即为通过）
        match &status.availability {
            RuntimeAvailability::Available { version } => {
                // version 可以为空，但不能是乱码
                let _ = version.len();
            }
            RuntimeAvailability::Unavailable { reason } => {
                // reason 不能为空
                prop_assert!(!reason.is_empty(), "Unavailable 原因不应为空，命令名: {}", cmd);
            }
        }
    }
}
