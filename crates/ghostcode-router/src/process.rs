// @file process.rs
// @description 异步子进程管理器，封装 tokio::process::Command 启动/监控/终止 AI CLI 工具
//              支持 stdin 传递长文本、超时控制、取消令牌终止、stdout 逐行捕获
//              集成执行期主权约束：spawn 前调用 enforce_execution 阻断非 Claude 写入操作
//              参考: ccg-workflow/codeagent-wrapper/executor.go - 进程执行和 stdin pipe 逻辑
// @author Atlas.oi
// @date 2026-03-04

use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

// 引入执行期主权约束函数
use crate::sovereignty::{enforce_execution, SovereigntyViolation};

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
    /// 执行期主权约束违规：非 Claude 后端尝试写入操作
    #[error("主权约束违规: {0}")]
    SovereigntyViolation(#[from] SovereigntyViolation),
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
    /// 执行命令并等待结果（向后兼容接口，以 "claude" 身份执行）
    ///
    /// 业务逻辑：
    /// 1. 启动子进程，设置 stdin/stdout/stderr 管道
    /// 2. 如果提供了 stdin_data，通过 stdin pipe 写入后关闭
    /// 3. 在 tokio::select! 中同时等待：进程完成 / 超时 / 取消令牌
    /// 4. 取消时发送 SIGTERM，等待 5s 后发 SIGKILL
    /// 5. 退出码非零时返回 ProcessFailed 错误
    ///
    /// 注意：此接口以 "claude" 身份绕过主权检查（历史兼容），
    ///       新代码应优先使用 run_command_as() 以明确指定后端。
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
        // 向后兼容：以 "claude" 身份执行（executor.rs 历史调用路径）
        Self::run_command_as("claude", command, args, stdin_data, timeout, cancel).await
    }

    /// 执行命令并等待结果（带主权约束的主接口）
    ///
    /// 业务逻辑：
    /// 0. 执行期主权约束检查（enforce_execution）：非 Claude 后端的写入操作在此阶段拦截
    /// 1. 启动子进程，设置 stdin/stdout/stderr 管道
    /// 2. 如果提供了 stdin_data，通过 stdin pipe 写入后关闭
    /// 3. 在 tokio::select! 中同时等待：进程完成 / 超时 / 取消令牌
    /// 4. 取消时发送 SIGTERM，等待 5s 后发 SIGKILL
    /// 5. 退出码非零时返回 ProcessFailed 错误
    ///
    /// @param backend_name - 发起命令的后端名称（用于主权约束检查，如 "claude"/"codex"）
    /// @param command - 可执行文件名称
    /// @param args - 命令行参数列表
    /// @param stdin_data - 可选的 stdin 数据（None 则不提供 stdin）
    /// @param timeout - 超时时间，超时后强制终止进程
    /// @param cancel - 取消令牌，触发后 SIGTERM → 5s → SIGKILL
    /// @returns Ok(ProcessOutput) - 成功执行的结果
    /// @returns Err(ProcessError) - 主权违规/超时/取消/失败/IO 错误
    pub async fn run_command_as(
        backend_name: &str,
        command: &str,
        args: &[&str],
        stdin_data: Option<&str>,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        Self::run_command_as_in(backend_name, command, args, stdin_data, timeout, cancel, None, None).await
    }

    /// 执行命令并等待结果（带工作目录、环境变量和主权约束的完整接口）
    ///
    /// 业务逻辑：
    /// 0. 执行期主权约束检查（enforce_execution）
    /// 1. 启动子进程，设置工作目录和环境变量
    /// 2. 如果提供了 stdin_data，通过 stdin pipe 写入后关闭
    /// 3. 在 tokio::select! 中同时等待：进程完成 / 超时 / 取消令牌
    /// 4. 退出码非零时返回 ProcessFailed 错误
    ///
    /// 参考: ccg-workflow/codeagent-wrapper/executor.go:972-978
    ///       Go 版本通过 loadMinimalEnvSettings() + cmd.SetEnv() 注入环境变量
    ///       Rust 版本通过 envs 参数注入后端专属环境变量（如 Gemini 的 NODE_OPTIONS）
    ///
    /// @param backend_name - 后端名称（用于主权约束检查）
    /// @param command - 可执行文件名称
    /// @param args - 命令行参数列表
    /// @param stdin_data - 可选的 stdin 数据
    /// @param timeout - 超时时间
    /// @param cancel - 取消令牌
    /// @param workdir - 可选的工作目录（所有后端统一通过 cmd.current_dir 设置）
    /// @param envs - 可选的环境变量列表，追加到子进程环境中（不清除已有环境）
    /// @returns Ok(ProcessOutput) - 成功执行的结果
    /// @returns Err(ProcessError) - 主权违规/超时/取消/失败/IO 错误
    pub async fn run_command_as_in(
        backend_name: &str,
        command: &str,
        args: &[&str],
        stdin_data: Option<&str>,
        timeout: Duration,
        cancel: CancellationToken,
        workdir: Option<&std::path::Path>,
        envs: Option<&[(&str, &str)]>,
    ) -> Result<ProcessOutput, ProcessError> {
        let start = std::time::Instant::now();

        // ============================================
        // 第零步：执行期主权约束检查
        // 在创建任何系统进程之前，验证该后端是否有权执行该命令。
        // 非 Claude 后端的写入型命令在此阶段直接拒绝，不创建子进程。
        // 将 &[&str] args 转换为 Vec<String> 以匹配 enforce_execution 接口
        // ============================================
        let args_owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        enforce_execution(backend_name, command, &args_owned)?;

        // ============================================
        // 第一步：构建并启动子进程
        // stdout/stderr 设为 pipe 以便读取
        // stdin 根据是否有 stdin_data 决定是否设为 pipe
        // ============================================
        let mut cmd = Command::new(command);
        cmd.args(args);

        // ============================================
        // 工作目录设置（参考 ccg-workflow/codeagent-wrapper/executor.go:980-984）
        // 所有后端统一通过 cmd.current_dir() 设置进程工作目录。
        // Codex 不再使用 -C 参数传递路径，因为 Codex CLI 会将 -C 路径
        // 写入 websocket header，非 ASCII 字符会导致 UTF-8 编码错误。
        // ============================================
        if let Some(dir) = workdir {
            cmd.current_dir(dir);
        }

        // ============================================
        // 环境变量注入（参考 ccg-workflow/codeagent-wrapper/executor.go:972-978）
        // 后端专属环境变量追加到子进程环境中，不清除已有环境变量。
        // 典型用例：Gemini 后端需要 NODE_OPTIONS=--max-old-space-size=8192
        //          防止 Gemini CLI 在大型项目中因文件扫描导致 OOM 崩溃。
        // ============================================
        if let Some(env_pairs) = envs {
            cmd.envs(env_pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        }

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        if stdin_data.is_some() {
            cmd.stdin(std::process::Stdio::piped());
        } else {
            cmd.stdin(std::process::Stdio::null());
        }

        let mut child = cmd.spawn()?;

        // ============================================
        // 第二步：并发启动 stdin 写入 + stdout/stderr 读取
        //
        // 关键修复：stdin 写入必须与 stdout/stderr 读取并发执行，
        // 否则大数据量时会产生管道死锁（经典 pipe deadlock）：
        //   1. Rust 写 stdin → OS 管道缓冲区满（通常 64KB）
        //   2. 子进程处理后写 stdout → stdout 管道缓冲区满（无人读取）
        //   3. 子进程阻塞在 stdout 写入 → 无法继续读 stdin
        //   4. Rust 阻塞在 stdin 写入 → 死锁
        //
        // 参考: ccg-workflow/codeagent-wrapper/executor.go:1089-1094
        //       Go 版本使用 goroutine 并发写 stdin，避免此问题
        // ============================================
        let stdout = child.stdout.take().expect("stdout 管道应该存在");
        let stderr = child.stderr.take().expect("stderr 管道应该存在");

        // stdin 写入任务：并发执行，写完后自动关闭（drop 发送 EOF）
        if let Some(data) = stdin_data {
            if let Some(mut stdin) = child.stdin.take() {
                let data_owned = data.to_string();
                tokio::spawn(async move {
                    if let Err(e) = stdin.write_all(data_owned.as_bytes()).await {
                        tracing::warn!("stdin 写入失败: {}", e);
                    }
                    // drop stdin 自动关闭管道，发送 EOF 给子进程
                });
            }
        }

        // 异步读取 stdout 的所有行（与 stdin 写入并发执行）
        let stdout_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            let mut lines = Vec::new();
            while let Ok(Some(line)) = reader.next_line().await {
                lines.push(line);
            }
            lines
        });

        // 异步读取 stderr 的全部内容（与 stdin 写入并发执行）
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
