// @file main.rs
// @description ghostcode-wrapper 薄 CLI 入口
//              复用 ghostcode-router 的所有模块，对外提供统一的 CLI 接口。
//              支持 Codex / Claude / Gemini 三种后端，
//              通过 --backend 参数选择，通过 stdin 或位置参数传递任务文本。
//
//              参考: ccg-workflow/codeagent-wrapper/main.go - CLI 入口和主流程
//
// @author Atlas.oi
// @date 2026-03-06

use std::io::Read;
use std::path::PathBuf;
use std::process;
use std::time::Duration;

use clap::{Parser, ValueEnum};
use tokio_util::sync::CancellationToken;
use tracing::error;

use ghostcode_router::backend::{
    Backend, ClaudeBackend, CodexBackend, GeminiBackend, TaskConfig, TaskMode,
};
use ghostcode_router::process::{ProcessManager, should_use_stdin};
use ghostcode_router::rolefile::inject_role_files;
use ghostcode_router::runtime_probe::{RuntimeAvailability, probe_runtime};
use ghostcode_router::stream::{StreamEventKind, StreamParser};

// ============================================================
// CLI 参数定义（clap derive 宏）
// ============================================================

/// 后端选择枚举（对应 --backend 参数的合法值）
#[derive(Debug, Clone, ValueEnum)]
enum BackendKind {
    /// OpenAI Codex CLI
    Codex,
    /// Google Gemini CLI
    Gemini,
    /// Anthropic Claude CLI
    Claude,
}

/// ghostcode-wrapper - AI 后端统一调用 CLI
///
/// 封装 Codex / Claude / Gemini 三种 AI CLI 工具，
/// 提供统一的参数接口和 JSON 流输出解析。
#[derive(Debug, Parser)]
#[command(name = "ghostcode-wrapper", version, about)]
struct Cli {
    /// 选择 AI 后端（codex / gemini / claude）
    #[arg(long, value_enum)]
    backend: BackendKind,

    /// 工作目录（默认为当前目录）
    #[arg(long, default_value = ".")]
    workdir: PathBuf,

    /// 超时时间（秒，默认 600）
    #[arg(long, default_value_t = 600u64)]
    timeout: u64,

    /// 模型名称（Gemini 使用 -m 参数传入，其他后端忽略）
    #[arg(long)]
    model: Option<String>,

    /// 从 stdin 读取任务文本（与位置参数互斥）
    /// 等价于传统 Unix 约定中的 `-` 位置参数
    #[arg(long = "stdin", conflicts_with = "task_text")]
    stdin_flag: bool,

    /// 任务文本（直接作为位置参数传入，与 `-` 互斥）
    task_text: Option<String>,
}

// ============================================================
// 主入口
// ============================================================

#[tokio::main]
async fn main() {
    // ============================================
    // 第一步：初始化结构化日志
    // 使用 try_init 避免多次初始化冲突（与其他 crate 共存）
    // ============================================
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .try_init();

    // ============================================
    // 第二步：解析 CLI 参数
    // ============================================
    let cli = Cli::parse();

    // ============================================
    // 第三步：构建 Backend 实例，获取命令名用于探测
    // ============================================
    let (backend_name, backend_command): (&'static str, &'static str) = match &cli.backend {
        BackendKind::Codex => ("codex", "codex"),
        BackendKind::Claude => ("claude", "claude"),
        BackendKind::Gemini => ("gemini", "gemini"),
    };

    // ============================================
    // 第四步：探测后端 CLI 是否可用
    // 不可用时打印错误并以 exit(127) 退出（命令未找到的约定退出码）
    // ============================================
    let runtime_status = probe_runtime(backend_command);
    if let RuntimeAvailability::Unavailable { ref reason } = runtime_status.availability {
        eprintln!("错误：后端 '{}' 不可用 - {}", backend_name, reason);
        process::exit(127);
    }

    // ============================================
    // 第五步：读取任务文本
    // 优先从 stdin（-flag 或未提供位置参数），其次从位置参数
    // ============================================
    let raw_task_text = if cli.stdin_flag {
        // 明确要求从 stdin 读取
        let mut buf = String::new();
        if let Err(e) = std::io::stdin().read_to_string(&mut buf) {
            eprintln!("错误：读取 stdin 失败: {}", e);
            process::exit(1);
        }
        buf
    } else if let Some(text) = cli.task_text {
        // 从位置参数读取
        text
    } else {
        // 两者都未提供时，尝试从 stdin 读取（管道模式）
        let mut buf = String::new();
        if let Err(e) = std::io::stdin().read_to_string(&mut buf) {
            eprintln!("错误：读取 stdin 失败: {}", e);
            process::exit(1);
        }
        buf
    };

    // ============================================
    // 第六步：注入 ROLE_FILE 内容
    // 扫描任务文本中的 ROLE_FILE: <path> 行并替换为文件内容
    // ============================================
    let task_text = match inject_role_files(&raw_task_text) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("错误：ROLE_FILE 注入失败: {}", e);
            process::exit(1);
        }
    };

    // ============================================
    // 第七步：判断任务文本的传递方式
    // should_use_stdin: 文本过长或含特殊字符时使用 stdin pipe
    // ============================================
    let use_stdin = should_use_stdin(&task_text);

    // ============================================
    // 第八步：构建 TaskConfig 和 Backend 实例，生成 CLI 参数
    // ============================================
    let workdir = if cli.workdir.as_os_str() == "." {
        // 将相对路径 "." 展开为绝对路径，避免后端工作目录混乱
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        cli.workdir.clone()
    };

    let config = TaskConfig {
        workdir,
        mode: TaskMode::New,
        session_id: None,
        model: cli.model.clone(),
        timeout: Duration::from_secs(cli.timeout),
    };

    // 根据 backend 类型构建具体实现，获取 args
    let args_vec: Vec<String> = match &cli.backend {
        BackendKind::Codex => CodexBackend.build_args(&config),
        BackendKind::Claude => ClaudeBackend.build_args(&config),
        BackendKind::Gemini => GeminiBackend::new(cli.model.clone()).build_args(&config),
    };

    // ============================================
    // 第九步：根据 use_stdin 决定文本传递方式
    // use_stdin=true: 文本通过 stdin pipe 传递
    // use_stdin=false: 文本追加到 args 末尾
    // ============================================
    let (final_args, stdin_data) = if use_stdin {
        // 通过 stdin 传递：保持 args 不变
        (args_vec, Some(task_text.clone()))
    } else {
        // 通过参数传递：将任务文本追加到 args 末尾
        let mut args = args_vec;
        args.push(task_text.clone());
        (args, None)
    };

    // ============================================
    // 第十步：执行 AI CLI 子进程
    // 使用 ProcessManager::run_command_as 支持主权约束检查
    // ============================================
    let cancel_token = CancellationToken::new();
    let timeout = Duration::from_secs(cli.timeout);

    // 将 Vec<String> 转换为 Vec<&str> 以匹配 run_command_as 接口
    let args_refs: Vec<&str> = final_args.iter().map(|s| s.as_str()).collect();
    let stdin_ref = stdin_data.as_deref();

    let output = match ProcessManager::run_command_as(
        backend_name,
        backend_command,
        &args_refs,
        stdin_ref,
        timeout,
        cancel_token,
    )
    .await
    {
        Ok(out) => out,
        Err(e) => {
            error!("执行失败: {}", e);
            eprintln!("错误：{}", e);
            process::exit(1);
        }
    };

    // ============================================
    // 第十一步：解析 stdout 流，收集 AgentMessage 内容
    // 使用 StreamParser 统一处理三种后端的 JSON 流格式
    // ============================================
    let mut parser = StreamParser::new();
    let mut agent_messages: Vec<String> = Vec::new();

    for line in &output.stdout_lines {
        if line.trim().is_empty() {
            continue;
        }
        match parser.parse_line(line) {
            Ok(Some(event)) => {
                if event.kind == StreamEventKind::AgentMessage {
                    if let Some(content) = event.content {
                        agent_messages.push(content);
                    }
                }
            }
            Ok(None) => {
                // 无法识别的行，静默跳过
            }
            Err(e) => {
                // 解析错误记录警告，继续处理后续行（不中断）
                tracing::warn!("流解析警告: {}", e);
            }
        }
    }

    // ============================================
    // 第十二步：将 AgentMessage 内容输出到 stdout
    // 多条消息之间用换行分隔
    // ============================================
    if agent_messages.is_empty() {
        // 没有解析到 AgentMessage 时，输出原始 stdout 作为降级回退
        // 注意：此处保留原始输出是功能性需要（非降级隐藏错误），
        //       因为部分后端可能直接输出纯文本而非 JSON 流
        println!("{}", output.stdout_lines.join("\n"));
    } else {
        println!("{}", agent_messages.join("\n"));
    }

    // ============================================
    // 第十三步：以子进程退出码退出
    // 保持退出码语义与原始 AI CLI 一致
    // ============================================
    process::exit(output.exit_code);
}
