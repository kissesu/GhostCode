import { KeywordType, KeywordMatch } from './types.js';

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
declare const KEYWORD_PATTERNS: Record<KeywordType, {
    regex: RegExp;
    priority: number;
}>;
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
declare function detectMagicKeywords(input: string): KeywordMatch[];
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
declare function resolveKeywordPriority(matches: KeywordMatch[]): KeywordMatch | null;

export { KEYWORD_PATTERNS, detectMagicKeywords, resolveKeywordPriority };
