/*
 * @file sovereignty.rs
 * @description 代码主权守卫模块
 *              控制哪些后端（AI 模型）可以直接写入文件系统。
 *              核心规则：外部模型（Codex/Gemini）对文件系统零写入权限，
 *              所有修改由 Claude 执行。外部模型的输出须经过审查后再由 Claude 执行。
 *
 *              执行期约束：在进程 spawn 前调用 enforce_execution() 检查写入意图，
 *              非 Claude 后端的写入操作在进程创建前即被拒绝。
 *
 *              参考: ccg-workflow/templates/commands/execute.md:208-237
 *              （Claude 独占写入 + 外部输出审查策略）
 *
 * @author Atlas.oi
 * @date 2026-03-04
 */

// ============================================
// 写入意图分类关键词
// 用于 classify_write_intent() 判断命令是否具有写入意图
// ============================================

/// 写入型命令关键词列表
///
/// 包含常见的文件系统写入操作动词，大小写不敏感匹配
const WRITE_KEYWORDS: &[&str] = &[
    "write",    // 直接写入
    "edit",     // 编辑文件
    "create",   // 创建文件/目录
    "delete",   // 删除文件/目录
    "remove",   // 移除文件/目录
    "modify",   // 修改内容
    "update",   // 更新文件
    "patch",    // 打补丁
    "mv",       // 移动文件（等价 move）
    "cp",       // 复制文件
    "chmod",    // 修改权限
];

/// 只读型命令关键词列表
///
/// 包含常见的文件系统读取操作动词，大小写不敏感匹配
const READ_KEYWORDS: &[&str] = &[
    "read",     // 读取文件
    "list",     // 列举目录
    "search",   // 搜索内容
    "find",     // 查找文件/符号
    "get",      // 获取信息
    "show",     // 显示内容
    "cat",      // 输出文件内容
    "ls",       // 列举目录（Unix 风格）
    "grep",     // 正则搜索
    "review",   // 审查/审核（无文件系统写入）
    "analyze",  // 分析操作（无文件系统写入）
    "diagnose", // 诊断操作（无文件系统写入）
    "diff",     // 差异比较（无文件系统写入）
];

/// 已知后端 CLI 名称列表
///
/// 当 command 是后端 CLI 的二进制名时，表示这是一个 wrapper 发起的后端调用。
/// 后端 CLI 自身运行在受限沙箱中（Codex 的 --dangerously-bypass-approvals-and-sandbox
/// 仅绕过 Codex 内部审批，不赋予文件系统写入权限），因此应放行。
///
/// 参考: ccg-workflow 不在 CLI 执行层做意图分类，
/// 权限隔离通过各后端自身的沙箱机制保证。
/// 来源: ccg-workflow/codeagent-wrapper/backend.go:84-145
const KNOWN_BACKEND_CLI_NAMES: &[&str] = &[
    "codex",   // OpenAI Codex CLI
    "gemini",  // Google Gemini CLI
    "claude",  // Anthropic Claude CLI
];

// ============================================
// 写入意图分类
// ============================================

/// 命令的写入意图分类
///
/// 用于 classify_write_intent() 的返回值，表示命令对文件系统的操作意图
#[derive(Debug, Clone, PartialEq)]
pub enum WriteIntent {
    /// 明确的写入操作（命令名包含写入关键词）
    Write,
    /// 只读操作（命令名包含只读关键词）
    ReadOnly,
    /// 无法分类（命令名不包含已知关键词，安全起见视为潜在写入）
    Unknown,
}

/// 对命令进行写入意图分类
///
/// 基于命令名中的关键词判断是否包含写入操作。
/// 匹配规则：
/// 1. 命令名包含写入关键词（大小写不敏感）→ Write
/// 2. 命令名包含只读关键词（大小写不敏感）→ ReadOnly
/// 3. 两者均不包含 → Unknown（安全起见，视为潜在写入）
///
/// # 参数
/// - `command`: 命令名称（如 "write_file"、"read_file"）
/// - `args`: 命令参数列表（当前版本仅基于命令名分类，预留参数扩展）
///
/// # 返回值
/// WriteIntent 枚举，表示写入意图级别
pub fn classify_write_intent(command: &str, _args: &[String]) -> WriteIntent {
    let cmd_lower = command.to_lowercase();

    // 优先检查写入关键词（安全优先，写入关键词权重更高）
    for keyword in WRITE_KEYWORDS {
        if cmd_lower.contains(keyword) {
            return WriteIntent::Write;
        }
    }

    // 再检查只读关键词
    for keyword in READ_KEYWORDS {
        if cmd_lower.contains(keyword) {
            return WriteIntent::ReadOnly;
        }
    }

    // 无法分类，返回 Unknown
    WriteIntent::Unknown
}

// ============================================
// 主权违规错误类型
// ============================================

/// 主权违规错误
///
/// 在执行期约束（enforce_execution）检查失败时返回，
/// 阻止非 Claude 后端的写入操作创建子进程
#[derive(Debug, thiserror::Error)]
pub enum SovereigntyViolation {
    /// 非 Claude 后端尝试执行写入操作被拦截
    #[error("后端 '{backend}' 不允许执行写入操作 '{command}': {reason}")]
    WriteBlocked {
        /// 发起写入请求的后端名称
        backend: String,
        /// 被拦截的命令名称
        command: String,
        /// 拦截原因说明
        reason: String,
    },
}

// ============================================
// 执行期约束检查（进程 spawn 前调用）
// ============================================

/// 执行期约束检查
///
/// 在进程 spawn（Command::new）之前调用，阻止非 Claude 后端执行写入操作。
/// 这是代码主权规则的执行期执行，与 review_output 的审查功能互补：
/// - review_output: 事后审查外部模型的输出内容
/// - enforce_execution: 事前阻断非 Claude 后端的写入型命令
///
/// # 判断逻辑
/// 1. backend 为 "claude" → 始终通过（Claude 是唯一写入者）
/// 2. WriteIntent::ReadOnly → 通过（只读操作对任何后端开放）
/// 3. WriteIntent::Write 或 Unknown → 拒绝（安全优先，未知意图视为写入）
///
/// # 参数
/// - `backend_name`: 发起命令的后端名称（大小写不敏感）
/// - `command`: 即将执行的命令名称
/// - `args`: 命令参数列表
///
/// # 返回值
/// - `Ok(())`: 通过检查，可以继续创建子进程
/// - `Err(SovereigntyViolation)`: 违规，禁止创建子进程
pub fn enforce_execution(
    backend_name: &str,
    command: &str,
    args: &[String],
) -> Result<(), SovereigntyViolation> {
    // Claude 后端始终通过：Claude 是代码主权的唯一写入者
    if backend_name.to_lowercase() == "claude" {
        return Ok(());
    }

    // 后端 CLI 二进制调用放行：
    // 当 command 是已知后端 CLI 名称时（如 wrapper 调用 codex/gemini CLI），
    // 表示这是 ghostcode-wrapper 发起的后端调用。后端 CLI 自身在沙箱中运行，
    // 文件系统隔离由各后端保证，GhostCode 无需在 process spawn 层重复阻断。
    //
    // 参考: ccg-workflow 不在 CLI 执行层做意图分类
    // 来源: ccg-workflow/codeagent-wrapper/backend.go:84-145
    let cmd_lower = command.to_lowercase();
    if KNOWN_BACKEND_CLI_NAMES.iter().any(|name| *name == cmd_lower) {
        return Ok(());
    }

    // 分类命令的写入意图
    let intent = classify_write_intent(command, args);

    match intent {
        // 只读命令：所有后端均可执行
        WriteIntent::ReadOnly => Ok(()),

        // 写入命令：非 Claude 后端直接拒绝
        WriteIntent::Write => Err(SovereigntyViolation::WriteBlocked {
            backend: backend_name.to_string(),
            command: command.to_string(),
            reason: format!(
                "检测到写入意图关键词，非 Claude 后端 '{}' 对文件系统无写入权限",
                backend_name
            ),
        }),

        // 未知命令：安全优先，非 Claude 后端默认拒绝
        WriteIntent::Unknown => Err(SovereigntyViolation::WriteBlocked {
            backend: backend_name.to_string(),
            command: command.to_string(),
            reason: format!(
                "命令 '{}' 意图未知，非 Claude 后端 '{}' 默认拒绝（安全优先原则）",
                command, backend_name
            ),
        }),
    }
}

// ============================================
// 危险操作模式列表（硬编码初版）
// 检测到这些模式时直接拒绝，不允许任何后端执行
// ============================================

/// 危险命令关键词列表
/// 包含常见的文件系统破坏、数据库危险操作等模式
const DANGEROUS_PATTERNS: &[&str] = &[
    "rm -rf",        // Unix 递归强制删除
    "del /f",        // Windows 强制删除
    "format",        // 磁盘格式化
    "DROP TABLE",    // SQL 删表
    "DROP DATABASE", // SQL 删库
    "sudo rm",       // 超权删除
    "mkfs",          // 文件系统格式化
    "dd if=",        // 磁盘覆写
    "shred",         // 安全删除（不可恢复）
    "truncate",      // 文件截断清空
];

// ============================================
// 操作审查结果
// ============================================

/// 外部模型输出的审查结果
///
/// 用于 review_output() 返回值，表示对外部模型输出的处置决定
#[derive(Debug, Clone, PartialEq)]
pub enum ReviewResult {
    /// 允许直接执行（写入者身份匹配，无需审查）
    Approved,

    /// 需要审核（外部模型输出，建议由 Claude 审核后执行）
    NeedsReview {
        /// 外部模型提供的建议内容，由 Claude 最终审核决定是否采纳
        suggestion: String,
    },

    /// 直接拒绝（检测到危险操作模式）
    Rejected {
        /// 拒绝原因，说明检测到的危险模式
        reason: String,
    },
}

// ============================================
// 代码主权守卫
// ============================================

/// 代码主权守卫
///
/// 控制文件系统写入权限。默认只有 Claude 有写入权限，
/// 外部模型（Codex/Gemini 等）的输出必须经过审查流程。
///
/// # 使用示例
///
/// ```rust
/// use ghostcode_router::sovereignty::SovereigntyGuard;
///
/// let guard = SovereigntyGuard::new();
/// assert!(guard.can_write("claude"));
/// assert!(!guard.can_write("codex"));
/// ```
pub struct SovereigntyGuard {
    /// 具有直接写入权限的后端名称（小写存储）
    write_actor: String,
}

impl SovereigntyGuard {
    /// 创建默认守卫
    ///
    /// 默认写入者为 "claude"，符合代码主权核心规则。
    pub fn new() -> Self {
        Self {
            write_actor: "claude".to_string(),
        }
    }

    /// 创建自定义写入者的守卫
    ///
    /// 允许在特殊场景下（如测试环境）指定其他后端为写入者。
    ///
    /// # 参数
    /// - `actor`: 具有写入权限的后端名称（大小写不敏感，内部统一转小写存储）
    pub fn with_write_actor(actor: &str) -> Self {
        Self {
            write_actor: actor.to_lowercase(),
        }
    }

    /// 检查指定后端是否有写入权限
    ///
    /// 业务逻辑：
    /// 1. 空字符串直接返回 false
    /// 2. 比较 backend_name（小写）与 write_actor（已小写存储）
    /// 3. 大小写不敏感
    ///
    /// # 参数
    /// - `backend_name`: 后端名称，支持大小写混合输入
    ///
    /// # 返回值
    /// - `true`: 后端有写入权限
    /// - `false`: 后端无写入权限
    pub fn can_write(&self, backend_name: &str) -> bool {
        can_write_fn(backend_name, &self.write_actor)
    }

    /// 审查外部模型的输出内容
    ///
    /// 业务逻辑：
    /// 1. 若 backend_name 与 write_actor 匹配 → Approved（直接放行）
    /// 2. 检测输出是否包含危险模式 → Rejected（直接拒绝）
    /// 3. 其他情况 → NeedsReview（提交人工审核，suggestion 为原始输出）
    ///
    /// # 参数
    /// - `backend_name`: 产生输出的后端名称
    /// - `output`: 后端输出的内容
    ///
    /// # 返回值
    /// ReviewResult 枚举，表示审查结论
    pub fn review_output(&self, backend_name: &str, output: &str) -> ReviewResult {
        // 写入者自身输出直接放行
        if can_write_fn(backend_name, &self.write_actor) {
            return ReviewResult::Approved;
        }

        // 检测危险操作模式（大小写不敏感比较）
        let output_lower = output.to_lowercase();
        for pattern in DANGEROUS_PATTERNS {
            if output_lower.contains(&pattern.to_lowercase()) {
                return ReviewResult::Rejected {
                    reason: format!(
                        "检测到危险操作模式 '{}' 于后端 '{}' 的输出中",
                        pattern, backend_name
                    ),
                };
            }
        }

        // 外部模型的普通输出，需要审核
        ReviewResult::NeedsReview {
            suggestion: output.to_string(),
        }
    }
}

impl Default for SovereigntyGuard {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================
// 纯函数：写入权限判断逻辑（便于测试和复用）
// ============================================

/// 判断后端是否有写入权限的纯函数
///
/// 从 can_write() 提取为纯函数，方便单独测试和复用。
///
/// # 参数
/// - `backend_name`: 后端名称（任意大小写）
/// - `write_actor`: 具有写入权限的写入者名称（已小写）
#[inline]
fn can_write_fn(backend_name: &str, write_actor: &str) -> bool {
    // 空字符串不允许写入
    if backend_name.is_empty() {
        return false;
    }
    backend_name.to_lowercase() == write_actor
}
