// @file runtime_probe.rs
// @description AI CLI 工具运行时探测模块
//              在路由执行前检查 CLI 工具（Codex/Claude/Gemini）是否存在于 PATH 中，
//              并尝试获取版本信息。探测操作使用同步 std::process::Command，
//              因为这是一次性操作，不需要异步。
//
//              核心逻辑：
//              1. 执行 `command --version` 获取版本输出
//              2. 命令不存在（NotFound）-> Unavailable，附带友好原因
//              3. 命令存在但执行失败 -> Unavailable，附带退出码或错误信息
//              4. 命令成功执行 -> Available，附带 stdout/stderr 首行作为版本信息
//
// @author Atlas.oi
// @date 2026-03-04

use std::process::Command;

// ============================================
// 公开类型定义
// ============================================

/// CLI 工具可用性状态
///
/// - Available: 命令存在且可执行，包含版本信息
/// - Unavailable: 命令不存在或执行失败，包含原因描述
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeAvailability {
    /// 命令可用，version 为 stdout/stderr 首行（可能为空字符串）
    Available { version: String },
    /// 命令不可用，reason 描述具体原因（NotFound、执行失败等）
    Unavailable { reason: String },
}

/// 单个 CLI 工具的运行时探测结果
#[derive(Debug, Clone)]
pub struct RuntimeStatus {
    /// 后端友好名称（如 "Codex"、"Claude"、"Gemini"）
    pub name: String,
    /// CLI 可执行文件名（如 "codex"、"claude"、"gemini"）
    pub command: String,
    /// 探测结果
    pub availability: RuntimeAvailability,
}

// ============================================
// 核心探测函数
// ============================================

/// 探测指定命令是否可用，并尝试获取版本信息
///
/// 业务逻辑：
/// 1. 执行 `{command} --version`，捕获 stdout 和 stderr
/// 2. 若命令不存在（io::ErrorKind::NotFound）-> Unavailable
/// 3. 若命令存在但执行出错 -> Unavailable + 退出码信息
/// 4. 若成功执行 -> Available + stdout 首行（优先）或 stderr 首行
///
/// @param command - CLI 可执行文件名（不含路径，从 PATH 查找）
/// @returns RuntimeStatus，name 字段设为与 command 相同的值
pub fn probe_runtime(command: &str) -> RuntimeStatus {
    // ============================================
    // 执行 `command --version` 并捕获输出
    // ============================================
    let result = Command::new(command).arg("--version").output();

    let availability = match result {
        Err(e) => {
            // 命令无法启动：最常见的是 NotFound（命令不在 PATH 中）
            // 其他可能：PermissionDenied 等
            let reason = match e.kind() {
                std::io::ErrorKind::NotFound => {
                    format!("命令 '{}' 不在 PATH 中或不存在", command)
                }
                std::io::ErrorKind::PermissionDenied => {
                    format!("命令 '{}' 没有执行权限", command)
                }
                _ => {
                    format!("无法启动命令 '{}': {}", command, e)
                }
            };
            RuntimeAvailability::Unavailable { reason }
        }
        Ok(output) => {
            if output.status.success() {
                // ============================================
                // 命令成功执行：提取版本信息
                // 优先从 stdout 提取首行，若为空则尝试 stderr
                // ============================================
                let version = extract_first_line(&output.stdout)
                    .or_else(|| extract_first_line(&output.stderr))
                    .unwrap_or_default();
                RuntimeAvailability::Available { version }
            } else {
                // ============================================
                // 命令存在但执行失败（非零退出码）
                // 某些工具在 --version 时返回非零退出码，仍视为可用
                // 但本实现保守处理：先尝试从输出提取版本
                // ============================================
                let version_from_output = extract_first_line(&output.stdout)
                    .or_else(|| extract_first_line(&output.stderr));

                match version_from_output {
                    Some(version) if !version.is_empty() => {
                        // 有版本输出，仍视为可用
                        RuntimeAvailability::Available { version }
                    }
                    _ => {
                        // 无任何输出，视为不可用
                        let exit_code = output
                            .status
                            .code()
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| "未知".to_string());
                        RuntimeAvailability::Unavailable {
                            reason: format!(
                                "命令 '{}' 执行失败，退出码: {}",
                                command, exit_code
                            ),
                        }
                    }
                }
            }
        }
    };

    RuntimeStatus {
        name: command.to_string(),
        command: command.to_string(),
        availability,
    }
}

/// 从字节切片中提取第一行非空文本
///
/// @param bytes - 命令输出的字节数组（stdout 或 stderr）
/// @returns 第一行非空文本，若无有效内容则返回 None
fn extract_first_line(bytes: &[u8]) -> Option<String> {
    // 将字节转换为字符串，忽略无效 UTF-8 字节
    let text = String::from_utf8_lossy(bytes);
    // 找到第一行非空内容
    text.lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
}

// ============================================
// 三个后端的辅助探测函数
// ============================================

/// 探测 Codex CLI 工具（OpenAI Codex）
///
/// @returns RuntimeStatus，name 为 "Codex"，command 为 "codex"
pub fn probe_codex() -> RuntimeStatus {
    let mut status = probe_runtime("codex");
    // 覆盖 name 字段为友好名称
    status.name = "Codex".to_string();
    status
}

/// 探测 Claude CLI 工具（Anthropic Claude）
///
/// @returns RuntimeStatus，name 为 "Claude"，command 为 "claude"
pub fn probe_claude() -> RuntimeStatus {
    let mut status = probe_runtime("claude");
    // 覆盖 name 字段为友好名称
    status.name = "Claude".to_string();
    status
}

/// 探测 Gemini CLI 工具（Google Gemini）
///
/// @returns RuntimeStatus，name 为 "Gemini"，command 为 "gemini"
pub fn probe_gemini() -> RuntimeStatus {
    let mut status = probe_runtime("gemini");
    // 覆盖 name 字段为友好名称
    status.name = "Gemini".to_string();
    status
}

/// 探测所有已知 AI CLI 工具，返回全部探测结果
///
/// 探测顺序：Codex -> Claude -> Gemini
/// 每个工具独立探测，互不影响
///
/// @returns Vec<RuntimeStatus>，长度固定为 3
pub fn probe_all() -> Vec<RuntimeStatus> {
    vec![probe_codex(), probe_claude(), probe_gemini()]
}
