/**
 * @file extractor.ts
 * @description Skill 模板提取器
 *              将 PatternDetection 转化为带 YAML frontmatter 的 Markdown 模板文本
 * @author Atlas.oi
 * @date 2026-03-03
 */

import type { PatternDetection } from "./types.js";

/**
 * 将候选模式提取为 Skill 模板文本（YAML frontmatter + Markdown 正文）
 *
 * @param pattern - 候选模式
 * @param skillId - Skill slug ID
 * @param skillName - 人类可读名称
 * @returns YAML frontmatter + Markdown 正文的完整文本
 */
export function extractSkillTemplate(
  pattern: PatternDetection,
  skillId: string,
  skillName: string,
): string {
  const triggers = pattern.suggestedTriggers.join(", ");
  const tags = pattern.suggestedTags.join(", ");
  const now = new Date().toISOString();

  const frontmatter = [
    "---",
    `id: ${skillId}`,
    `name: "${skillName}"`,
    `description: "${pattern.problem.slice(0, 100).replace(/"/g, "'")}"`,
    `triggers: "${triggers}"`,
    `source: extracted`,
    `version: "1.0.0"`,
    `quality: "${pattern.confidence}"`,
    `usageCount: "0"`,
    `tags: ${tags}`,
    `createdAt: ${now}`,
    "---",
  ].join("\n");

  const body = [
    `# ${skillName}`,
    "",
    "## 问题",
    pattern.problem,
    "",
    "## 解决方案",
    pattern.solution,
    "",
    `> 置信度: ${pattern.confidence}/100 | 观察次数: ${pattern.occurrences}`,
  ].join("\n");

  return `${frontmatter}\n${body}`;
}
