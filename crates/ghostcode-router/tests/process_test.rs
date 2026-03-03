// @file process_test.rs
// @description 异步子进程管理器 process.rs 的集成测试
//              遵循 TDD 流程：先写测试（Red），再写实现（Green），最后重构（Refactor）
//              测试覆盖：should_use_stdin 纯函数 + run() 进程执行逻辑
// @author Atlas.oi
// @date 2026-03-02

use ghostcode_router::process::{should_use_stdin, ProcessError, ProcessManager};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

// ============================================================
// should_use_stdin 纯函数测试
// 验证何时应该通过 stdin 传递任务文本（而非命令行参数）
// ============================================================

#[test]
fn should_use_stdin_for_long_text() {
    // 超过 800 字节的文本必须使用 stdin，避免命令行长度限制
    let long_text = "a".repeat(801);
    assert!(
        should_use_stdin(&long_text),
        "超过 800 字节的文本应该使用 stdin"
    );
}

#[test]
fn should_use_stdin_for_special_chars() {
    // 含换行符的文本必须使用 stdin，命令行参数无法安全传递多行文本
    let text_with_newline = "第一行\n第二行";
    assert!(
        should_use_stdin(text_with_newline),
        "含 \\n 的文本应该使用 stdin"
    );

    // 含其他特殊字符也应使用 stdin
    let text_with_backslash = "path\\to\\file";
    assert!(
        should_use_stdin(text_with_backslash),
        "含反斜杠的文本应该使用 stdin"
    );

    let text_with_dollar = "price is $100";
    assert!(
        should_use_stdin(text_with_dollar),
        "含 $ 的文本应该使用 stdin"
    );
}

#[test]
fn should_not_use_stdin_for_short_text() {
    // 短文本且无特殊字符可以直接作为命令行参数传递
    let short_text = "简单的短文本任务";
    assert!(
        !should_use_stdin(short_text),
        "短文本且无特殊字符不应该使用 stdin"
    );

    // 恰好 800 字节时不使用 stdin（边界值：> 800 才使用）
    let exactly_800 = "a".repeat(800);
    assert!(
        !should_use_stdin(&exactly_800),
        "恰好 800 字节不应该使用 stdin（需要 > 800 才触发）"
    );
}

// ============================================================
// ProcessManager::run() 异步集成测试
// 使用系统命令（echo/cat/sleep/false）模拟真实 CLI 行为
// ============================================================

#[tokio::test]
async fn run_echo_captures_stdout() {
    // 验证 ProcessManager 能正确捕获子进程的 stdout 输出
    // 使用 echo 命令作为最简单的测试替身
    let cancel = CancellationToken::new();
    let result = ProcessManager::run_command(
        "echo",
        &["hello ghostcode"],
        None, // 不通过 stdin 传递
        Duration::from_secs(5),
        cancel,
    )
    .await;

    assert!(result.is_ok(), "echo 命令应该成功执行");
    let output = result.unwrap();
    assert_eq!(output.exit_code, 0, "echo 命令退出码应为 0");
    // stdout 应包含 "hello ghostcode"
    let stdout_text: String = output
        .stdout_lines
        .iter()
        .map(|l| l.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        stdout_text.contains("hello ghostcode"),
        "stdout 应包含 echo 输出的内容，实际得到: {:?}",
        stdout_text
    );
}

#[tokio::test]
async fn timeout_kills_process() {
    // 验证超时机制：sleep 100 应该在 1s 后被终止并返回 Timeout 错误
    let cancel = CancellationToken::new();
    let result = ProcessManager::run_command(
        "sleep",
        &["100"],
        None,
        Duration::from_secs(1), // 1 秒超时
        cancel,
    )
    .await;

    assert!(result.is_err(), "sleep 100 应该因超时而失败");
    assert!(
        matches!(result.unwrap_err(), ProcessError::Timeout(_)),
        "应该返回 Timeout 错误"
    );
}

#[tokio::test]
async fn nonzero_exit_returns_error() {
    // 验证非零退出码处理：false 命令始终以退出码 1 退出
    let cancel = CancellationToken::new();
    let result = ProcessManager::run_command(
        "false",
        &[],
        None,
        Duration::from_secs(5),
        cancel,
    )
    .await;

    assert!(result.is_err(), "false 命令应该失败");
    match result.unwrap_err() {
        ProcessError::ProcessFailed { exit_code, .. } => {
            assert_eq!(exit_code, 1, "false 命令退出码应为 1");
        }
        other => panic!("期望 ProcessFailed 错误，实际得到: {:?}", other),
    }
}

#[tokio::test]
async fn cancel_stops_process() {
    // 验证取消令牌能正确终止长时间运行的子进程
    let cancel = CancellationToken::new();

    // 在另一个任务中触发取消，稍微延迟让进程先启动
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        cancel_clone.cancel();
    });

    let result = ProcessManager::run_command(
        "sleep",
        &["100"],
        None,
        Duration::from_secs(10), // 超时设长，确保是取消而非超时触发
        cancel,
    )
    .await;

    assert!(result.is_err(), "被取消的进程应该返回错误");
    assert!(
        matches!(result.unwrap_err(), ProcessError::Cancelled),
        "应该返回 Cancelled 错误"
    );
}
