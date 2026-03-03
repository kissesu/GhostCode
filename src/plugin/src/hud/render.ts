/**
 * @file HUD 状态栏渲染器
 * @description 将 HudSnapshot 渲染为完整的状态栏字符串
 *              组合所有元素，用 " | " 分隔，前缀 "[GC] "
 *              参考: oh-my-claudecode - HUD 状态栏设计思路
 * @author Atlas.oi
 * @date 2026-03-03
 */
import { renderRalph } from "./elements/ralph";
import { renderContext } from "./elements/context";
import { renderCost } from "./elements/cost";
import { renderModel } from "./elements/model";
import { renderAgent } from "./elements/agent";
import { CYAN, RESET } from "./colors";
import type { HudSnapshot, StatuslineOptions } from "./types";

/**
 * 将 HudSnapshot 渲染为完整的 HUD 状态栏字符串
 *
 * 业务逻辑：
 * 1. 渲染各个元素（ralph、context、cost、model、agents）
 * 2. 过滤掉空字符串元素（如无验证时 ralph 为空）
 * 3. 用 " | " 连接所有非空元素
 * 4. 添加 "[GC] " 前缀标识（使用青色）
 *
 * 输出示例：[GC] ralph:3/10 | ctx:72% | cost:$0.15 | model:opus | agents:2
 *
 * @param snapshot - HUD 状态快照数据
 * @param options - 可选的渲染配置（模型名称、颜色开关等）
 * @returns 渲染后的状态栏字符串
 */
export function renderStatusline(
  snapshot: HudSnapshot,
  options: StatuslineOptions = {}
): string {
  const { modelName = "unknown", useColors = true } = options;

  // ============================================
  // 渲染各元素
  // 无验证时 renderRalph 返回空字符串，会被过滤掉
  // ============================================
  const ralphEl = renderRalph(snapshot.verification);
  const contextEl = renderContext(snapshot.context_pressure);
  const costEl = renderCost(snapshot.cost);
  const modelEl = renderModel(modelName);
  const agentEl = renderAgent(snapshot.active_agents);

  // ============================================
  // 过滤空元素，拼接为状态栏内容
  // ============================================
  const elements = [ralphEl, contextEl, costEl, modelEl, agentEl].filter(
    (el) => el.length > 0
  );

  const content = elements.join(" | ");

  // ============================================
  // 添加 [GC] 前缀，根据 useColors 决定是否着色
  // useColors=false 时去除所有 ANSI 转义码（用于测试断言）
  // ============================================
  const prefix = useColors ? `${CYAN}[GC]${RESET}` : "[GC]";
  const line = `${prefix} ${content}`;

  if (!useColors) {
    // 去除所有 ANSI 转义码，返回纯文本
    return line.replace(/\x1b\[[0-9;]*m/g, "");
  }

  return line;
}
