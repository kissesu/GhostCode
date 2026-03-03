// @file backend.rs
// @description Backend trait 定义和三后端 CLI 参数构建器实现
//              参考 ccg-workflow/codeagent-wrapper/backend.go 和 executor.go 的实现逻辑
//              将 TaskConfig 转换为对应 AI 工具的命令行参数列表
//
//              参考溯源:
//              - ccg-workflow/codeagent-wrapper/backend.go:13-17 - Backend interface 定义
//              - ccg-workflow/codeagent-wrapper/executor.go:757-799 - buildCodexArgs 实现
//              - ccg-workflow/codeagent-wrapper/backend.go:84-108 - buildClaudeArgs 实现
//              - ccg-workflow/codeagent-wrapper/backend.go:120-145 - buildGeminiArgs 实现
//
// @author Atlas.oi
// @date 2026-03-02

use std::path::PathBuf;
use std::time::Duration;

// ============================================
// Codex 后端公共安全标志常量
// 默认开启：绕过审批沙箱 + 跳过 git 仓库检查
// ============================================

/// Codex 绕过审批和沙箱的安全标志
/// 参考: ccg-workflow/codeagent-wrapper/executor.go:777
const CODEX_BYPASS_FLAG: &str = "--dangerously-bypass-approvals-and-sandbox";

/// Codex 跳过 git 仓库检查的标志
/// 参考: ccg-workflow/codeagent-wrapper/executor.go:782
const CODEX_SKIP_GIT_FLAG: &str = "--skip-git-repo-check";

/// Codex JSON 输出格式标志
const CODEX_JSON_FLAG: &str = "--json";

/// Claude 跳过权限检查的安全标志
/// 参考: ccg-workflow/codeagent-wrapper/backend.go:91
const CLAUDE_SKIP_PERMISSIONS_FLAG: &str = "--dangerously-skip-permissions";

// ============================================
// 任务模式枚举
// ============================================

/// 任务执行模式
/// - New: 创建新会话
/// - Resume: 恢复已有会话
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskMode {
    /// 新任务模式：从头开始执行
    New,
    /// 恢复模式：继续已有的 AI 会话
    Resume,
}

// ============================================
// 任务配置结构体
// ============================================

/// 任务配置，包含执行 AI 工具所需的所有参数
#[derive(Debug, Clone)]
pub struct TaskConfig {
    /// 工作目录路径（Codex new 模式使用 -C 参数传入）
    pub workdir: PathBuf,
    /// 任务执行模式（New / Resume）
    pub mode: TaskMode,
    /// 会话 ID（Resume 模式必须提供）
    pub session_id: Option<String>,
    /// AI 模型名称（Gemini 使用 -m 参数传入）
    pub model: Option<String>,
    /// 任务超时时间
    pub timeout: Duration,
}

// ============================================
// Backend Trait 定义
// ============================================

/// AI 后端 CLI 参数构建器 trait
/// 每个实现负责根据 TaskConfig 生成对应工具的命令行参数列表
/// 参考: ccg-workflow/codeagent-wrapper/backend.go:13-17 - Backend interface
pub trait Backend {
    /// 返回后端名称（用于日志和错误信息）
    fn name(&self) -> &'static str;

    /// 返回后端可执行文件命令名
    fn command(&self) -> &'static str;

    /// 根据任务配置构建命令行参数列表
    ///
    /// @param config - 任务配置，包含模式、工作目录、会话 ID 等信息
    /// @returns 命令行参数列表（不含可执行文件名本身）
    fn build_args(&self, config: &TaskConfig) -> Vec<String>;
}

// ============================================
// CodexBackend 实现
// ============================================

/// Codex AI 工具后端
/// 参考: ccg-workflow/codeagent-wrapper/executor.go:757-799 - buildCodexArgs 实现
#[derive(Debug, Clone)]
pub struct CodexBackend;

impl Backend for CodexBackend {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn command(&self) -> &'static str {
        "codex"
    }

    /// 构建 Codex 命令行参数
    ///
    /// 参数构建逻辑：
    /// - new 模式：codex e --dangerously-bypass-approvals-and-sandbox --skip-git-repo-check -C workdir --json
    /// - resume 模式：codex e --dangerously-bypass-approvals-and-sandbox --skip-git-repo-check --json resume session_id
    ///
    /// 参考: ccg-workflow/codeagent-wrapper/executor.go:757-799
    fn build_args(&self, config: &TaskConfig) -> Vec<String> {
        // 基础参数：子命令 e（exec）+ 安全标志
        let mut args = vec![
            "e".to_string(),
            CODEX_BYPASS_FLAG.to_string(),
            CODEX_SKIP_GIT_FLAG.to_string(),
        ];

        match &config.mode {
            TaskMode::Resume => {
                // Resume 模式：不传 -C workdir，改为传 resume session_id
                // 参考: ccg-workflow/codeagent-wrapper/executor.go:785-791
                if let Some(session_id) = &config.session_id {
                    args.push(CODEX_JSON_FLAG.to_string());
                    args.push("resume".to_string());
                    args.push(session_id.clone());
                } else {
                    // session_id 为空时，退化为 new 模式处理（暴露问题而非隐藏）
                    args.push(CODEX_JSON_FLAG.to_string());
                    args.push("-C".to_string());
                    args.push(config.workdir.to_string_lossy().to_string());
                }
            }
            TaskMode::New => {
                // New 模式：传入 -C workdir 指定工作目录
                // 参考: ccg-workflow/codeagent-wrapper/executor.go:793-798
                args.push("-C".to_string());
                args.push(config.workdir.to_string_lossy().to_string());
                args.push(CODEX_JSON_FLAG.to_string());
            }
        }

        args
    }
}

// ============================================
// ClaudeBackend 实现
// ============================================

/// Claude AI 工具后端
/// 参考: ccg-workflow/codeagent-wrapper/backend.go:84-108 - buildClaudeArgs 实现
#[derive(Debug, Clone)]
pub struct ClaudeBackend;

impl Backend for ClaudeBackend {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn command(&self) -> &'static str {
        "claude"
    }

    /// 构建 Claude 命令行参数
    ///
    /// 参数构建逻辑：
    /// - new 模式：claude -p --dangerously-skip-permissions --setting-sources "" --output-format stream-json --verbose
    /// - resume 模式：claude -p --dangerously-skip-permissions --setting-sources "" -r session_id --output-format stream-json --verbose
    ///
    /// 注意：Claude CLI 不支持 -C 标志，工作目录通过进程 cmd.Dir 设置
    /// 参考: ccg-workflow/codeagent-wrapper/backend.go:84-108
    fn build_args(&self, config: &TaskConfig) -> Vec<String> {
        // 基础参数：-p（print 模式）+ 权限跳过 + 禁用所有 setting sources
        // 禁用 setting sources 的目的：防止无限递归调用（避免 CLAUDE.md 或 skills 触发 codeagent）
        // 参考: ccg-workflow/codeagent-wrapper/backend.go:93-95
        let mut args = vec![
            "-p".to_string(),
            CLAUDE_SKIP_PERMISSIONS_FLAG.to_string(),
            "--setting-sources".to_string(),
            "".to_string(),
        ];

        // Resume 模式追加 -r session_id
        if let TaskMode::Resume = &config.mode {
            if let Some(session_id) = &config.session_id {
                args.push("-r".to_string());
                args.push(session_id.clone());
            }
        }

        // 输出格式：流式 JSON + 详细日志
        args.push("--output-format".to_string());
        args.push("stream-json".to_string());
        args.push("--verbose".to_string());

        args
    }
}

// ============================================
// GeminiBackend 实现
// ============================================

/// Gemini AI 工具后端
/// 参考: ccg-workflow/codeagent-wrapper/backend.go:120-145 - buildGeminiArgs 实现
#[derive(Debug, Clone)]
pub struct GeminiBackend {
    /// 默认模型名称（可由 TaskConfig.model 覆盖）
    default_model: Option<String>,
}

impl GeminiBackend {
    /// 创建新的 GeminiBackend 实例
    ///
    /// @param default_model - 默认模型名称，None 时不传 -m 参数
    pub fn new(default_model: Option<String>) -> Self {
        Self { default_model }
    }
}

impl Backend for GeminiBackend {
    fn name(&self) -> &'static str {
        "gemini"
    }

    fn command(&self) -> &'static str {
        "gemini"
    }

    /// 构建 Gemini 命令行参数
    ///
    /// 参数构建逻辑：
    /// - new 模式：gemini -m model -o stream-json -y -p
    /// - resume 模式：gemini -m model -o stream-json -y -r session_id -p
    ///
    /// 注意：Gemini CLI 也不支持 -C 标志，工作目录通过进程 cmd.Dir 设置
    /// 参考: ccg-workflow/codeagent-wrapper/backend.go:120-145
    fn build_args(&self, config: &TaskConfig) -> Vec<String> {
        let mut args = Vec::new();

        // 模型参数（优先使用 TaskConfig 中的 model，其次使用 default_model）
        // 参考: ccg-workflow/codeagent-wrapper/backend.go:128-130
        let model = config
            .model
            .as_deref()
            .filter(|m| !m.trim().is_empty())
            .or_else(|| self.default_model.as_deref().filter(|m| !m.trim().is_empty()));

        if let Some(m) = model {
            args.push("-m".to_string());
            args.push(m.to_string());
        }

        // 输出格式 stream-json + 自动确认（-y）
        // 参考: ccg-workflow/codeagent-wrapper/backend.go:133
        args.push("-o".to_string());
        args.push("stream-json".to_string());
        args.push("-y".to_string());

        // Resume 模式追加 -r session_id
        if let TaskMode::Resume = &config.mode {
            if let Some(session_id) = &config.session_id {
                args.push("-r".to_string());
                args.push(session_id.clone());
            }
        }

        // -p 标志（print/prompt 模式）
        args.push("-p".to_string());

        args
    }
}
