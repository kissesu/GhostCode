/**
 * @file detector.ts
 * @description 会话模式检测器
 *              基于启发式规则从会话内容中识别可复用的问题-解答模式
 * @author Atlas.oi
 * @date 2026-03-03
 */

import type { PatternDetection } from "./types.js";
import { createHash } from "node:crypto";

// 最小内容长度阈值（字符数），低于此值不触发检测
const MIN_CONTENT_LENGTH = 100;

// 高价值关键词（提高置信度）
const HIGH_VALUE_KEYWORDS = [
  "错误", "error", "修复", "fix", "解决",
  "solved", "resolved", "方案", "solution",
];

// 错误模式匹配
const ERROR_PATTERNS = [
  /(?:TypeError|ReferenceError|SyntaxError|Error)[:：]\s*([^\n]+)/gi,
  /(?:错误|问题|issue)[:：]\s*([^\n]+)/gi,
  /cannot find|找不到|not found|未找到/gi,
];

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
export function detectPatterns(content: string): PatternDetection[] {
  if (content.length < MIN_CONTENT_LENGTH) {
    return [];
  }

  const patterns: PatternDetection[] = [];
  const lower = content.toLowerCase();

  // 计算高价值关键词密度
  let keywordCount = 0;
  for (const kw of HIGH_VALUE_KEYWORDS) {
    const matches = lower.split(kw).length - 1;
    keywordCount += matches;
  }

  // 检测错误修复模式
  let errorMatches = 0;
  for (const pattern of ERROR_PATTERNS) {
    const matches = content.match(pattern);
    if (matches) {
      errorMatches += matches.length;
    }
  }

  // 只有同时具有错误模式和解决方案迹象时才创建候选
  if (errorMatches > 0 && keywordCount >= 2) {
    const confidence = Math.min(50 + keywordCount * 5 + errorMatches * 10, 100);
    const now = new Date().toISOString();
    const id = createHash("sha256")
      .update(content.slice(0, 200))
      .digest("hex")
      .slice(0, 16);

    patterns.push({
      id,
      problem: content.slice(0, 200).trim(),
      solution: content.slice(-200).trim(),
      confidence,
      occurrences: 1,
      firstSeen: now,
      lastSeen: now,
      suggestedTriggers: extractTriggers(content),
      suggestedTags: extractTags(content),
    });
  }

  return patterns;
}

/** 从内容中提取触发词建议 */
function extractTriggers(content: string): string[] {
  const triggers: string[] = [];
  if (/rust|cargo/i.test(content)) triggers.push("rust");
  if (/typescript|ts\b/i.test(content)) triggers.push("typescript");
  if (/python|pip/i.test(content)) triggers.push("python");
  if (/error|错误/i.test(content)) triggers.push("fix");
  return triggers.slice(0, 3);
}

/** 从内容中提取标签建议 */
function extractTags(content: string): string[] {
  const tags: string[] = [];
  if (/rust/i.test(content)) tags.push("rust");
  if (/typescript/i.test(content)) tags.push("typescript");
  if (/fix|修复/i.test(content)) tags.push("bugfix");
  return tags.slice(0, 3);
}
