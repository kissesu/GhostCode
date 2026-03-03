/**
 * @file types.ts
 * @description Learner 模块类型定义
 *              与 Rust 侧 ghostcode_types::skill 类型对齐
 *              参考: oh-my-claudecode/src/hooks/learner/types.ts
 * @author Atlas.oi
 * @date 2026-03-03
 */

/** Skill 来源类型 */
export type SkillSource = "extracted" | "promoted" | "manual";

/** Skill 作用域 */
export type SkillScope = "user" | "project";

/** Skill 元数据（YAML frontmatter 结构） */
export interface SkillMetadata {
  id: string;
  name: string;
  description: string;
  triggers: string[];
  createdAt: string;
  source: SkillSource;
  quality: number;
  usageCount: number;
  tags: string[];
}

/** 已学习的 Skill 文件 */
export interface LearnedSkill {
  path: string;
  relativePath: string;
  scope: SkillScope;
  metadata: SkillMetadata;
  content: string;
  contentHash: string;
  priority: number;
}

/** 候选 Skill 模式（待确认） */
export interface PatternDetection {
  id: string;
  problem: string;
  solution: string;
  confidence: number;
  occurrences: number;
  firstSeen: string;
  lastSeen: string;
  suggestedTriggers: string[];
  suggestedTags: string[];
}

/** 会话片段（上报给 daemon 的数据） */
export interface SessionFragment {
  problem: string;
  solution: string;
  confidence: number;
  context: string;
  suggestedTriggers: string[];
  suggestedTags: string[];
}
