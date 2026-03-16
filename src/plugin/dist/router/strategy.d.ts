import { RouteDecision } from './types.js';

/**
 * @file 路由策略实现
 * @description 根据任务文本自动判断目标后端（关键词匹配 + 强制前缀覆盖）
 * @author Atlas.oi
 * @date 2026-03-02
 */

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
declare function resolveRoute(taskText: string): RouteDecision;

export { resolveRoute };
