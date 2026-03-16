import { PatternDetection } from './types.js';

/**
 * @file extractor.ts
 * @description Skill 模板提取器
 *              将 PatternDetection 转化为带 YAML frontmatter 的 Markdown 模板文本
 * @author Atlas.oi
 * @date 2026-03-03
 */

/**
 * 将候选模式提取为 Skill 模板文本（YAML frontmatter + Markdown 正文）
 *
 * @param pattern - 候选模式
 * @param skillId - Skill slug ID
 * @param skillName - 人类可读名称
 * @returns YAML frontmatter + Markdown 正文的完整文本
 */
declare function extractSkillTemplate(pattern: PatternDetection, skillId: string, skillName: string): string;

export { extractSkillTemplate };
