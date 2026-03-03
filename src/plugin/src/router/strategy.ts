/**
 * @file 路由策略实现
 * @description 根据任务文本自动判断目标后端（关键词匹配 + 强制前缀覆盖）
 * @author Atlas.oi
 * @date 2026-03-02
 */

import type { BackendName, RouteDecision } from './types';

// 前端关键词（匹配到 → gemini）
const FRONTEND_KEYWORDS: string[] = [
  'css', 'html', 'ui', 'ux', 'style', 'component',
  'layout', 'responsive', 'design', 'animation',
];

// 后端关键词（匹配到 → codex）
const BACKEND_KEYWORDS: string[] = [
  'api', 'database', 'db', 'sql', 'algorithm',
  'server', 'backend', 'logic', 'auth', 'middleware',
];

// 强制前缀正则：匹配 /codex、/claude、/gemini 开头
const FORCED_PREFIX = /^\/(codex|claude|gemini)\b/i;

/**
 * 解析路由决策
 *
 * 优先级：强制前缀 > 关键词匹配 > 默认 claude
 *
 * 业务逻辑说明：
 * 1. 检查任务文本是否以 /codex、/claude、/gemini 开头（强制覆盖，置信度 1）
 * 2. 统计前端/后端关键词匹配数量，得分高者胜出（置信度按匹配数量比例计算）
 * 3. 无匹配时默认路由到 claude（置信度 0）
 *
 * @param taskText - 任务文本
 * @returns 路由决策结果，包含目标后端、原因、置信度
 */
export function resolveRoute(taskText: string): RouteDecision {
  // ============================================
  // 第一步：强制前缀检查
  // 用户显式指定后端，优先级最高，置信度固定为 1
  // ============================================
  const prefixMatch = taskText.match(FORCED_PREFIX);
  if (prefixMatch && prefixMatch[1]) {
    const matched = prefixMatch[1];
    return {
      backend: matched.toLowerCase() as BackendName,
      reason: `强制前缀 /${matched}`,
      confidence: 1,
    };
  }

  const lower = taskText.toLowerCase();

  // ============================================
  // 第二步：关键词匹配
  // 统计前端/后端关键词命中数，得分高者决定路由目标
  // 置信度 = min(命中数 * 0.3, 0.9)，最高不超过 0.9
  // ============================================
  const frontendScore = FRONTEND_KEYWORDS.filter(kw => lower.includes(kw)).length;
  const backendScore = BACKEND_KEYWORDS.filter(kw => lower.includes(kw)).length;

  if (frontendScore > 0 && frontendScore >= backendScore) {
    return {
      backend: 'gemini',
      reason: `前端关键词匹配 (${frontendScore} 个)`,
      confidence: Math.min(frontendScore * 0.3, 0.9),
    };
  }

  if (backendScore > 0) {
    return {
      backend: 'codex',
      reason: `后端关键词匹配 (${backendScore} 个)`,
      confidence: Math.min(backendScore * 0.3, 0.9),
    };
  }

  // ============================================
  // 第三步：默认路由
  // 无任何关键词命中，交给 claude 处理（通用任务）
  // ============================================
  return {
    backend: 'claude',
    reason: '无匹配关键词，默认路由',
    confidence: 0,
  };
}
