/**
 * @file parser.ts
 * @description Magic Keywords 解析器
 * 检测用户提示词中的魔法关键词，支持优先级解析和防误检
 *
 * 参考: oh-my-claudecode/src/hooks/keyword-detector/index.ts:46-72
 * - KEYWORD_PATTERNS 关键词正则表达式映射
 * - KEYWORD_PRIORITY 优先级排序
 * - detectKeywordsWithType 检测逻辑
 *
 * @author Atlas.oi
 * @date 2026-03-03
 */

import type { KeywordMatch, KeywordType } from "./types.js";
import { sanitizeForKeywordDetection } from "./sanitize.js";

/**
 * 关键词正则表达式和优先级映射
 *
 * 优先级规则：数字越小优先级越高
 * - cancel: 1（取消操作，最高优先级）
 * - ralph: 2（Ralph 验证循环）
 * - autopilot: 3（自动驾驶模式，含变体）
 * - team: 4（团队协作模式，排除冠词修饰）
 * - ultrawork: 5（极致工作模式）
 */
export const KEYWORD_PATTERNS: Record<
  KeywordType,
  { regex: RegExp; priority: number }
> = {
  // cancel：匹配 cancel 关键词（精确词边界）
  cancel: {
    regex: /\b(cancel)\b/i,
    priority: 1,
  },

  // ralph：匹配 ralph，排除 ralph-xxx 连字符形式（如 ralph-mode）
  // 参考: oh-my-claudecode/src/hooks/keyword-detector/index.ts:48
  ralph: {
    regex: /\b(ralph)\b(?!-)/i,
    priority: 2,
  },

  // autopilot：匹配多种变体
  // - autopilot（基本形式）
  // - auto-pilot（连字符形式）
  // - auto pilot（空格形式）
  // - full auto（全自动表达）
  // 参考: oh-my-claudecode/src/hooks/keyword-detector/index.ts:49
  autopilot: {
    regex: /\b(autopilot|auto[\s-]?pilot|full\s+auto)\b/i,
    priority: 3,
  },

  // team：匹配 team，但排除冠词/代词修饰的常见表达
  // 排除：my team, the team, our team, his team, her team, their team, a team, its team
  // 参考: oh-my-claudecode/src/hooks/keyword-detector/index.ts:53
  team: {
    regex: /(?<!\b(?:my|the|our|a|his|her|their|its)\s)\bteam\b/i,
    priority: 4,
  },

  // ultrawork：匹配 ultrawork 及其缩写 ulw
  // 参考: oh-my-claudecode/src/hooks/keyword-detector/index.ts:51
  ultrawork: {
    regex: /\b(ultrawork|ulw)\b/i,
    priority: 5,
  },
};

/**
 * 关键词优先级排序（从高到低）
 * 用于多关键词冲突时确定处理顺序
 */
const KEYWORD_PRIORITY_ORDER: KeywordType[] = [
  "cancel",
  "ralph",
  "autopilot",
  "team",
  "ultrawork",
];

/**
 * 检测输入文本中的所有 Magic Keywords
 *
 * 业务逻辑：
 * 1. 先对输入进行 sanitize 处理，移除代码块、URL、路径等噪声
 * 2. 按优先级顺序依次检测每种关键词
 * 3. 返回所有匹配到的关键词列表
 *
 * @param input - 用户输入的原始提示词
 * @returns 所有匹配到的关键词列表（可能包含多个）
 */
export function detectMagicKeywords(input: string): KeywordMatch[] {
  // 先清理噪声，再进行关键词检测
  const cleanedText = sanitizeForKeywordDetection(input);
  const matches: KeywordMatch[] = [];

  for (const type of KEYWORD_PRIORITY_ORDER) {
    const { regex, priority } = KEYWORD_PATTERNS[type];
    const matchResult = cleanedText.match(regex);

    if (matchResult !== null) {
      matches.push({
        type,
        priority,
        match: matchResult[0],
      });
    }
  }

  return matches;
}

/**
 * 根据优先级从多个关键词匹配中选出最高优先级的一个
 *
 * 业务逻辑：
 * 1. 如果没有匹配项，返回 null
 * 2. 按 priority 数值升序排列，返回第一个（数字最小 = 优先级最高）
 *
 * @param matches - detectMagicKeywords 返回的匹配列表
 * @returns 优先级最高的关键词匹配，或 null
 */
export function resolveKeywordPriority(
  matches: KeywordMatch[]
): KeywordMatch | null {
  if (matches.length === 0) {
    return null;
  }

  // 按优先级数值升序排列，取最小值（优先级最高）
  const sorted = [...matches].sort((a, b) => a.priority - b.priority);
  return sorted[0] ?? null;
}
