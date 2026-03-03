//! Skill Learning 类型定义
//!
//! 与 oh-my-claudecode learner/types.ts 对齐的 Rust 类型
//! 用于 Skill 候选管理、元数据存储和质量评分
//!
//! @author Atlas.oi
//! @date 2026-03-03

use serde::{Deserialize, Serialize};

/// Skill 来源类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillSource {
    /// 从会话自动提取
    Extracted,
    /// 用户手动确认提升
    Promoted,
    /// 用户手动创建
    Manual,
}

/// Skill 作用域
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope {
    /// 用户全局 Skill（~/.claude/skills/）
    User,
    /// 项目级 Skill（.claude/skills/）
    Project,
}

/// Skill 元数据（YAML frontmatter 结构）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillMetadata {
    /// 唯一标识符（slug 格式，如 "fix-rust-lifetime"）
    pub id: String,
    /// 人类可读名称
    pub name: String,
    /// 功能描述
    pub description: String,
    /// 触发关键词列表
    pub triggers: Vec<String>,
    /// 创建时间戳（ISO 8601）
    pub created_at: String,
    /// 来源类型
    pub source: SkillSource,
    /// 质量分（0-100）
    pub quality: u8,
    /// 使用次数
    pub usage_count: u32,
    /// 标签列表（可选）
    pub tags: Vec<String>,
}

/// 已学习的 Skill 文件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedSkill {
    /// 绝对路径
    pub path: String,
    /// 相对于 skills 目录的路径
    pub relative_path: String,
    /// 作用域
    pub scope: SkillScope,
    /// 元数据
    pub metadata: SkillMetadata,
    /// Skill 正文内容（YAML frontmatter 之后的 Markdown 内容）
    pub content: String,
    /// 内容 SHA-256 hash（用于去重）
    pub content_hash: String,
    /// 优先级（project > user）
    pub priority: u8,
}

/// 候选 Skill 模式（待确认）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternDetection {
    /// 候选唯一 ID
    pub id: String,
    /// 问题描述（从会话中提取）
    pub problem: String,
    /// 解决方案描述
    pub solution: String,
    /// 置信度（0-100，>= 70 才显示给用户）
    pub confidence: u8,
    /// 观察到此模式的次数
    pub occurrences: u32,
    /// 第一次观察时间戳
    pub first_seen: String,
    /// 最后观察时间戳
    pub last_seen: String,
    /// 建议的触发关键词
    pub suggested_triggers: Vec<String>,
    /// 建议的标签
    pub suggested_tags: Vec<String>,
}

impl SkillMetadata {
    /// 验证 quality 在有效范围内（0-100）
    pub fn is_quality_valid(&self) -> bool {
        self.quality <= 100
    }
}
