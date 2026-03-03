/*
 * @file sovereignty.rs
 * @description 代码主权守卫模块
 *              控制哪些后端（AI 模型）可以直接写入文件系统。
 *              核心规则：外部模型（Codex/Gemini）对文件系统零写入权限，
 *              所有修改由 Claude 执行。外部模型的输出须经过审查后再由 Claude 执行。
 *
 *              参考: ccg-workflow/templates/commands/execute.md:208-237
 *              （Claude 独占写入 + 外部输出审查策略）
 *
 * @author Atlas.oi
 * @date 2026-03-02
 */

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
