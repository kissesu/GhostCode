/**
 * @file index.ts
 * @description Learner 模块公共导出
 * @author Atlas.oi
 * @date 2026-03-03
 */

export type {
  LearnedSkill,
  SkillMetadata,
  PatternDetection,
  SessionFragment,
  SkillSource,
  SkillScope,
} from "./types.js";
export { detectPatterns } from "./detector.js";
export { extractSkillTemplate } from "./extractor.js";
export { writeSkillFile } from "./writer.js";
export { loadSkillsFromDir } from "./loader.js";
export { onSessionEnd, appendSessionContent, listCandidates } from "./manager.js";
