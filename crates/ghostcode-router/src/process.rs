// @file process.rs
// @description 异步子进程管理器，封装 tokio::process::Command 启动/监控/终止 AI CLI 工具
//              支持 stdin 传递长文本、超时控制、取消令牌终止、stdout 逐行捕获
//              参考: ccg-workflow/codeagent-wrapper/executor.go - 进程执行和 stdin pipe 逻辑
// @author Atlas.oi
// @date 2026-03-02

use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

// ============================================================
// 公开类型定义
// ============================================================

/// 子进程执行结果
#[derive(Debug)]
pub struct ProcessOutput {
    /// 进程退出码
    pub exit_code: i32,
    /// stdout 逐行捕获（每行为一个字符串）
    pub stdout_lines: Vec<String>,
    /// stderr 全文内容
    pub stderr: String,
    /// 实际执行时长
    pub duration: Duration,
}

/// 子进程管理错误
#[derive(thiserror::Error, Debug)]
pub enum ProcessError {
    /// 进程以非零退出码结束
    #[error("进程执行失败，退出码: {exit_code}, stderr: {stderr}")]
    ProcessFailed { exit_code: i32, stderr: String },
    /// 进程执行超时被强制终止
    #[error("进程执行超时（{0:?}）")]
    Timeout(Duration),
    /// 进程被取消令牌终止
    #[error("进程被取消")]
    Cancelled,
    /// IO 错误（进程启动失败等）
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
}

// ============================================================
// should_use_stdin 纯函数
// ============================================================

/// 判断是否应该通过 stdin 传递任务文本
///
/// 业务逻辑：
/// 1. 文本长度 > 800 字节时，命令行参数可能超出 shell/OS 限制，使用 stdin
/// 2. 含特殊字符（\n \\ " ' ` $）时，命令行转义容易出错，使用 stdin
///
/// @param text - 需要传递给 AI CLI 的任务文本
/// @returns true = 使用 stdin pipe 传递；false = 作为命令行参数传递
pub fn should_use_stdin(text: &str) -> bool {
    // 长度超限检查：> 800 字节使用 stdin
    if text.len() > 800 {
        return true;
    }

    // 特殊字符检查：任一出现即使用 stdin
    // 这些字符在 shell 命令行中有特殊含义，转义处理容易引入 bug
    let special_chars = ['\n', '\\', '"', '\'', '`', '$'];
    text.chars().any(|c| special_chars.contains(&c))
}

// ============================================================
// ProcessManager 子进程管理器实现
// ============================================================

/// 子进程管理器
///
/// 封装 tokio::process::Command，提供：
/// - 超时控制（tokio::time::timeout）
/// - 取消令牌支持（CancellationToken → SIGTERM → 5s → SIGKILL）
/// - stdout 逐行捕获
/// - 非零退出码转 ProcessFailed 错误
pub struct ProcessManager;

impl ProcessManager {
    /// 执行命令并等待结果（测试友好接口）
    ///
    /// 业务逻辑：
    /// 1. 启动子进程，设置 stdin/stdout/stderr 管道
    /// 2. 如果提供了 stdin_data，通过 stdin pipe 写入后关闭
    /// 3. 在 tokio::select! 中同时等待：进程完成 / 超时 / 取消令牌
    /// 4. 取消时发送 SIGTERM，等待 5s 后发 SIGKILL
    /// 5. 退出码非零时返回 ProcessFailed 错误
    ///
    /// @param command - 可执行文件名称
    /// @param args - 命令行参数列表
    /// @param stdin_data - 可选的 stdin 数据（None 则不提供 stdin）
    /// @param timeout - 超时时间，超时后强制终止进程
    /// @param cancel - 取消令牌，触发后 SIGTERM → 5s → SIGKILL
    /// @returns Ok(ProcessOutput) - 成功执行的结果
    /// @returns Err(ProcessError) - 超时/取消/失败/IO 错误
    pub async fn run_command(
        command: &str,
        args: &[&str],
        stdin_data: Option<&str>,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        let start = std::time::Instant::now();

        // ============================================
        // 第一步：构建并启动子进程
        // stdout/stderr 设为 pipe 以便读取
        // stdin 根据是否有 stdin_data 决定是否设为 pipe
        // ============================================
        let mut cmd = Command::new(command);
        cmd.args(args);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        if stdin_data.is_some() {
            cmd.stdin(std::process::Stdio::piped());
        } else {
            cmd.stdin(std::process::Stdio::null());
        }

        let mut child = cmd.spawn()?;

        // ============================================
        // 第二步：如果有 stdin_data，写入后关闭 stdin
        // 必须在等待进程之前关闭，否则子进程会一直等待 EOF
        // ============================================
        if let Some(data) = stdin_data {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(data.as_bytes()).await?;
                // drop 会自动关闭 stdin，发送 EOF 给子进程
            }
        }

        // ============================================
        // 第三步：并发读取 stdout 并等待进程结束
        // 使用 select! 同时监控超时和取消令牌
        // ============================================
        let stdout = child.stdout.take().expect("stdout 管道应该存在");
        let stderr = child.stderr.take().expect("stderr 管道应该存在");

        // 异步读取 stdout 的所有行
        let stdout_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            let mut lines = Vec::new();
            while let Ok(Some(line)) = reader.next_line().await {
                lines.push(line);
            }
            lines
        });

        // 异步读取 stderr 的全部内容
        let stderr_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            let mut content = String::new();
            while let Ok(Some(line)) = reader.next_line().await {
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(&line);
            }
            content
        });

        // ============================================
        // 第四步：select! 等待进程结束、超时或取消
        // 按优先级：取消 > 超时 > 正常结束
        // ============================================
        let status = tokio::select! {
            // 正常等待进程结束
            result = child.wait() => {
                match result {
                    Ok(status) => status,
                    Err(e) => return Err(ProcessError::Io(e)),
                }
            }
            // 超时：强制终止
            _ = tokio::time::sleep(timeout) => {
                // 先尝试 SIGTERM（优雅关闭），再 SIGKILL（强制）
                kill_process(&mut child).await;
                return Err(ProcessError::Timeout(timeout));
            }
            // 取消令牌触发：发 SIGTERM，等待 5s 后 SIGKILL
            _ = cancel.cancelled() => {
                terminate_with_grace(&mut child).await;
                return Err(ProcessError::Cancelled);
            }
        };

        // ============================================
        // 第五步：收集 stdout/stderr 结果
        // ============================================
        let stdout_lines = stdout_task.await.unwrap_or_default();
        let stderr_content = stderr_task.await.unwrap_or_default();
        let duration = start.elapsed();
        let exit_code = status.code().unwrap_or(-1);

        // 非零退出码 → ProcessFailed 错误（暴露问题，不做降级处理）
        if exit_code != 0 {
            return Err(ProcessError::ProcessFailed {
                exit_code,
                stderr: stderr_content,
            });
        }

        Ok(ProcessOutput {
            exit_code,
            stdout_lines,
            stderr: stderr_content,
            duration,
        })
    }
}

// ============================================================
// 内部辅助函数：进程终止
// ============================================================

/// 强制终止进程（SIGKILL）
///
/// 用于超时场景，直接发送 SIGKILL 不等待优雅关闭
async fn kill_process(child: &mut tokio::process::Child) {
    // 忽略 kill 错误（进程可能已经结束）
    let _ = child.kill().await;
}

/// 优雅终止进程（SIGTERM → 5s → SIGKILL）
///
/// 用于取消令牌场景，先发 SIGTERM 给进程机会清理，
/// 等待 5 秒后如果还在运行则 SIGKILL 强制终止
///
/// 参考: ccg-workflow/codeagent-wrapper/executor.go 信号处理逻辑
async fn terminate_with_grace(child: &mut tokio::process::Child) {
    // 第一步：发送 SIGTERM（Unix 专用，给进程优雅清理的机会）
    // 使用 nix::sys::signal 或 std::os::unix 原始接口发送 SIGTERM
    // tokio 的 child.kill() 在 Unix 上只发 SIGKILL，不支持 SIGTERM
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            // 通过 kill(2) 系统调用发送 SIGTERM
            // 安全性：pid 来自已知的子进程，信号值为标准常量
            unsafe {
                // SIGTERM = 15（POSIX 标准）
                libc::kill(pid as i32, 15);
            }
        }
    }

    // 非 Unix 平台直接 kill（Windows 无 SIGTERM 概念）
    #[cfg(not(unix))]
    {
        let _ = child.kill().await;
        return;
    }

    // 第二步：等待进程在 5s 内自然退出，超时则 SIGKILL 强制终止
    let grace_period = tokio::time::sleep(Duration::from_secs(5));
    tokio::select! {
        // 进程在优雅期内自行退出
        _ = child.wait() => {}
        // 5s 超时，SIGKILL 强制终止
        _ = grace_period => {
            let _ = child.kill().await;
        }
    }
}
