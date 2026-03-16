import { PatternDetection } from './types.js';

/**
 * @file detector.ts
 * @description 会话模式检测器
 *              基于启发式规则从会话内容中识别可复用的问题-解答模式
 * @author Atlas.oi
 * @date 2026-03-03
 */

/**
 * 从会话内容中检测可复用的模式
 *
 * 业务逻辑：
 * 1. 内容过短直接返回空数组
 * 2. 检测错误修复模式（高频关键词 + 错误模式）
 * 3. 根据关键词密度计算置信度
 * 4. 返回置信度 >= 0 的候选（调用方负责过滤）
 *
 * @param content - 会话内容文本
 * @returns 检测到的候选模式列表
 */
declare function detectPatterns(content: string): PatternDetection[];

export { detectPatterns };
