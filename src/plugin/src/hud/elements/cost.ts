/**
 * @file 成本元素渲染器
 * @description 将成本汇总数据渲染为 HUD 状态栏中的 cost 元素
 *              单位转换：micro-cents -> 美元（除以 100_000_000）
 * @author Atlas.oi
 * @date 2026-03-03
 */
import { DIM, colorize } from "../colors";
import type { CostSummary } from "../types";

/**
 * micro-cents 到美元的换算系数
 * 1 美元 = 100_000_000 micro-cents
 */
const MICRO_CENTS_PER_DOLLAR = 100_000_000;

/**
 * 渲染成本元素
 *
 * 业务逻辑：
 * - 将 total_cost_micro（micro-cents）转换为美元
 * - 格式：cost:$${amount.toFixed(2)}（保留两位小数）
 * - 使用暗色显示，属于次要信息
 *
 * @param cost - 成本汇总数据
 * @returns 格式化后的 cost 元素字符串
 */
export function renderCost(cost: CostSummary): string {
  const dollars = cost.total_cost_micro / MICRO_CENTS_PER_DOLLAR;
  const text = `cost:$${dollars.toFixed(2)}`;
  return colorize(text, DIM);
}
