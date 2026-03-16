import { LearnedSkill } from './types.js';

/**
 * @file loader.ts
 * @description Skill 文件加载器
 *              从指定目录加载所有 .md 格式的 Skill 文件
 * @author Atlas.oi
 * @date 2026-03-03
 */

/**
 * 从指定目录加载所有 Skill 文件
 *
 * @param dir - Skill 目录路径
 * @returns 已加载的 Skill 列表
 */
declare function loadSkillsFromDir(dir: string): Promise<LearnedSkill[]>;

export { loadSkillsFromDir };
