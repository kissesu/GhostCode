//! Skill Learning 核心引擎
//!
//! 负责从 TS 层上报的会话片段中提取可复用的 Skill 候选，
//! 通过质量门（confidence >= 70）过滤，去重后存储为待确认候选。
//! 用户确认后通过 promote_skill 提升为正式 LearnedSkill。
//!
//! 数据流：
//! TS Plugin -> ingest_session_fragment -> 候选存储
//!           -> list_skill_candidates -> 用户查看
//!           -> promote_skill -> LearnedSkill 存储
//!
//! @author Atlas.oi
//! @date 2026-03-03

use std::collections::HashMap;

use ghostcode_types::skill::{
    LearnedSkill, PatternDetection, SkillMetadata, SkillScope, SkillSource,
};
use uuid::Uuid;

// ============================================
// 公共类型定义
// ============================================

/// 会话片段（由 TS Plugin 采集并上报）
///
/// 代表一次完整的"问题-解答"对，是 Skill 候选的原始数据
#[derive(Debug, Clone)]
pub struct SessionFragment {
    /// 问题描述（从会话中提取的上下文）
    pub problem: String,
    /// 解决方案描述
    pub solution: String,
    /// 置信度评分（0-100），由 TS 层预评估
    pub confidence: u8,
    /// 原始上下文（用于 Skill 正文生成）
    pub context: String,
    /// 建议的触发关键词
    pub suggested_triggers: Vec<String>,
    /// 建议的标签
    pub suggested_tags: Vec<String>,
}

/// Skill 候选存储（内存状态，随 Daemon 生命周期）
///
/// 使用 HashMap 以 (problem, solution) 的 hash 为键实现去重
pub struct SkillStore {
    /// 待确认的候选，key 为去重 hash
    candidates_map: HashMap<String, PatternDetection>,
    /// 已提升的 Skill 列表
    promoted: Vec<LearnedSkill>,
}

impl SkillStore {
    /// 创建新的 Skill 存储实例
    pub fn new() -> Self {
        Self {
            candidates_map: HashMap::new(),
            promoted: Vec::new(),
        }
    }

    /// 获取所有候选（按 first_seen 排序）
    pub fn candidates(&self) -> Vec<&PatternDetection> {
        let mut v: Vec<&PatternDetection> = self.candidates_map.values().collect();
        v.sort_by(|a, b| a.first_seen.cmp(&b.first_seen));
        v
    }

    /// 获取所有已提升的 Skill
    pub fn promoted_skills(&self) -> &[LearnedSkill] {
        &self.promoted
    }
}

impl Default for SkillStore {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================
// 核心函数
// ============================================

/// 计算 (problem, solution) 的去重 hash（使用简单 hash 而非加密 hash）
fn dedup_key(problem: &str, solution: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    problem.hash(&mut hasher);
    solution.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

/// 摄入会话片段，若置信度达标则创建或更新候选
///
/// 业务逻辑：
/// 1. confidence < 70 -> 直接丢弃，返回 None
/// 2. 计算 (problem, solution) 去重 hash
/// 3. 若已存在相同 hash -> 更新 occurrences + last_seen，返回更新后的候选
/// 4. 若不存在 -> 创建新候选，插入存储，返回新候选
///
/// @param store - Skill 存储（可变）
/// @param fragment - 会话片段
/// @returns 若创建/更新了候选则返回 Some，否则返回 None
pub fn ingest_session_fragment(
    store: &mut SkillStore,
    fragment: SessionFragment,
) -> Option<PatternDetection> {
    // 质量门：低置信度内容直接丢弃
    if fragment.confidence < 70 {
        return None;
    }

    let key = dedup_key(&fragment.problem, &fragment.solution);
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    if let Some(existing) = store.candidates_map.get_mut(&key) {
        // 已存在：更新观察次数和最后时间
        existing.occurrences += 1;
        existing.last_seen = now;
        // confidence 取最大值（不降级）
        if fragment.confidence > existing.confidence {
            existing.confidence = fragment.confidence;
        }
        Some(existing.clone())
    } else {
        // 新候选
        let candidate = PatternDetection {
            id: Uuid::new_v4().simple().to_string(),
            problem: fragment.problem,
            solution: fragment.solution,
            confidence: fragment.confidence,
            occurrences: 1,
            first_seen: now.clone(),
            last_seen: now,
            suggested_triggers: fragment.suggested_triggers,
            suggested_tags: fragment.suggested_tags,
        };
        store.candidates_map.insert(key, candidate.clone());
        Some(candidate)
    }
}

/// 列出所有置信度达标的候选（只读）
///
/// @param store - Skill 存储
/// @returns 候选列表（按 first_seen 升序）
pub fn list_skill_candidates(store: &SkillStore) -> Vec<PatternDetection> {
    store.candidates().into_iter().cloned().collect()
}

/// 将候选提升为正式 LearnedSkill
///
/// 业务逻辑：
/// 1. 根据 candidate_id 查找候选
/// 2. 未找到 -> 返回 Err
/// 3. 构建 LearnedSkill（source=Promoted，scope=User）
/// 4. 从候选列表移除，加入 promoted 列表
/// 5. 返回 Ok(LearnedSkill)
///
/// @param store - Skill 存储（可变）
/// @param candidate_id - 候选 ID
/// @param skill_id - 新 Skill 的 slug ID
/// @param skill_name - 新 Skill 的人类可读名称
/// @returns Ok(LearnedSkill) 或 Err(错误描述)
pub fn promote_skill(
    store: &mut SkillStore,
    candidate_id: &str,
    skill_id: &str,
    skill_name: &str,
) -> Result<LearnedSkill, String> {
    // 找到对应候选的 key
    let key = store
        .candidates_map
        .iter()
        .find(|(_, c)| c.id == candidate_id)
        .map(|(k, _)| k.clone())
        .ok_or_else(|| format!("候选 '{}' 不存在", candidate_id))?;

    let candidate = store.candidates_map.remove(&key).unwrap();

    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    // quality 上限为 100
    let quality = candidate.confidence.min(100);

    let metadata = SkillMetadata {
        id: skill_id.to_string(),
        name: skill_name.to_string(),
        description: candidate.problem.clone(),
        triggers: candidate.suggested_triggers.clone(),
        created_at: now,
        source: SkillSource::Promoted,
        quality,
        usage_count: 0,
        tags: candidate.suggested_tags.clone(),
    };

    let content = format!(
        "# {}\n\n## 问题\n{}\n\n## 解决方案\n{}\n",
        skill_name, candidate.problem, candidate.solution
    );

    // 计算内容 hash（用于去重）
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    let content_hash = format!("{:x}", hasher.finish());

    let skill = LearnedSkill {
        path: format!("~/.claude/skills/{}.md", skill_id),
        relative_path: format!("{}.md", skill_id),
        scope: SkillScope::User,
        metadata,
        content,
        content_hash,
        priority: 10,
    };

    store.promoted.push(skill.clone());
    Ok(skill)
}
