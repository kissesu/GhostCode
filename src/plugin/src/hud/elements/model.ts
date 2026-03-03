/**
 * @file 模型名称元素渲染器
 * @description 渲染当前使用的模型名称，显示在 HUD 状态栏中
 *              模型名称不在 HudSnapshot 中，从外部参数传入
 * @author Atlas.oi
 * @date 2026-03-03
 */
import { CYAN, colorize } from "../colors";

/**
 * 渲染模型名称元素
 *
 * 业务逻辑：
 * - 格式：model:${name}
 * - 使用青色显示，突出模型标识
 * - 如果传入空字符串，显示 model:unknown
 *
 * @param modelName - 模型名称，如 "opus"、"sonnet"，默认 "unknown"
 * @returns 格式化后的 model 元素字符串
 */
export function renderModel(modelName: string = "unknown"): string {
  const name = modelName.trim() || "unknown";
  const text = `model:${name}`;
  return colorize(text, CYAN);
}
