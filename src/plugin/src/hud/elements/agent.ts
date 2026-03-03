/**
 * @file 活跃 Agent 数量元素渲染器
 * @description 渲染当前活跃 Agent 数量，显示在 HUD 状态栏中
 * @author Atlas.oi
 * @date 2026-03-03
 */
import { GREEN, DIM, colorize } from "../colors";

/**
 * 渲染活跃 Agent 数量元素
 *
 * 业务逻辑：
 * - 格式：agents:${count} 或 agents:?
 * - 有活跃 Agent 时：绿色显示，表示系统运行中
 * - 零 Agent 时：暗色显示，表示空闲状态
 * - null 时：暗色显示 "agents:?"，表示无法获取（RwLock 被占用）
 *
 * @param activeAgents - 活跃 Agent 数量，null 表示无法获取
 * @returns 格式化后的 agents 元素字符串
 */
export function renderAgent(activeAgents: number | null): string {
  if (activeAgents === null) {
    return colorize("agents:?", DIM);
  }
  const text = `agents:${activeAgents}`;
  const color = activeAgents > 0 ? GREEN : DIM;
  return colorize(text, color);
}
