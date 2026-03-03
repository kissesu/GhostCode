/**
 * @file HUD ANSI 颜色常量与工具函数
 * @description 提供终端 ANSI 颜色码常量及根据阈值选取颜色的工具函数
 * @author Atlas.oi
 * @date 2026-03-03
 */

// ============================================
// ANSI 颜色码常量
// 标准 8 色 ANSI 转义序列，兼容大多数现代终端
// ============================================

/** 重置所有属性（包括颜色） */
export const RESET = "\x1b[0m";

/** 绿色：表示状态正常 / 安全 */
export const GREEN = "\x1b[32m";

/** 黄色：表示警告 / 需要注意 */
export const YELLOW = "\x1b[33m";

/** 红色：表示危险 / 严重问题 */
export const RED = "\x1b[31m";

/** 青色：用于通用标签/前缀标记 */
export const CYAN = "\x1b[36m";

/** 暗色（低亮度）：用于次要信息 */
export const DIM = "\x1b[2m";

// ============================================
// 颜色辅助函数
// ============================================

/**
 * 根据百分比阈值选择对应的 ANSI 颜色码
 *
 * 业务逻辑：
 * - percentage > 85：危险区间，返回红色
 * - percentage >= 70：警告区间，返回黄色
 * - 其他：安全区间，返回绿色
 *
 * @param percentage - 百分比值（0.0 - 100.0）
 * @returns 对应的 ANSI 颜色码字符串
 */
export function pickColorByThreshold(percentage: number): string {
  if (percentage > 85) return RED;
  if (percentage >= 70) return YELLOW;
  return GREEN;
}

/**
 * 将文本包裹在指定颜色中，并自动追加 RESET 码
 *
 * @param text - 要着色的文本内容
 * @param color - ANSI 颜色码常量
 * @returns 带颜色码的文本字符串
 */
export function colorize(text: string, color: string): string {
  return `${color}${text}${RESET}`;
}
