/**
 * @file 上下文压力元素渲染器
 * @description 将上下文 Token 压力状态渲染为 HUD 状态栏中的 ctx 元素
 * @author Atlas.oi
 * @date 2026-03-03
 */
import { pickColorByThreshold, colorize } from "../colors";
import type { ContextPressure } from "../types";

/**
 * 渲染上下文压力元素
 *
 * 业务逻辑：
 * - 根据 percentage 值选择颜色（阈值：70 / 85）
 * - 格式：ctx:${percentage}%（百分比取整显示）
 * - 颜色根据压力等级选择：绿/黄/红
 *
 * @param pressure - 上下文压力状态数据
 * @returns 带颜色的 ctx 元素字符串
 */
export function renderContext(pressure: ContextPressure): string {
  const { percentage } = pressure;
  const color = pickColorByThreshold(percentage);
  const text = `ctx:${Math.round(percentage)}%`;
  return colorize(text, color);
}
