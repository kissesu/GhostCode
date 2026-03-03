/**
 * @file HUD 模块统一导出
 * @description 将 HUD 模块的所有公共 API 统一对外导出
 *              包括：类型定义、颜色工具、元素渲染器、状态栏渲染器、快照获取器
 * @author Atlas.oi
 * @date 2026-03-03
 */

// 类型定义
export type {
  ColorLevel,
  HudElement,
  VerificationStatus,
  VerificationSummary,
  CostSummary,
  ContextPressure,
  HudSnapshot,
  StatuslineOptions,
} from "./types";

// 颜色工具
export {
  RESET,
  GREEN,
  YELLOW,
  RED,
  CYAN,
  DIM,
  pickColorByThreshold,
  colorize,
} from "./colors";

// 元素渲染器
export {
  renderRalph,
  renderContext,
  renderCost,
  renderModel,
  renderAgent,
} from "./elements";

// 状态栏渲染器
export { renderStatusline } from "./render";

// 快照获取器
export { fetchHudSnapshot } from "./snapshot";
