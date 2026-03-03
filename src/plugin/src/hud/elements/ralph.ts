/**
 * @file Ralph 验证状态元素渲染器
 * @description 将验证状态摘要渲染为 HUD 状态栏中的 ralph 元素
 *              参考: oh-my-claudecode/src/skills/ralph.ts - Ralph 验证循环概念
 * @author Atlas.oi
 * @date 2026-03-03
 */
import { GREEN, RED, DIM, colorize } from "../colors";
import type { VerificationSummary } from "../types";

/**
 * 渲染 Ralph 验证状态元素
 *
 * 业务逻辑：
 * - 无验证（null）：返回空字符串，由上层过滤掉该元素
 * - Running：显示当前迭代进度 ralph:iteration/max_iterations
 * - Approved：绿色显示 ralph:OK（验证通过）
 * - Rejected：红色显示 ralph:FAIL（验证失败）
 * - Cancelled：暗色显示 ralph:--（验证已取消）
 *
 * @param verification - 验证状态摘要，无验证时为 null
 * @returns 渲染后的 HUD 元素字符串，无验证时为空字符串
 */
export function renderRalph(verification: VerificationSummary | null): string {
  if (verification === null) {
    return "";
  }

  const { status, iteration, max_iterations } = verification;

  switch (status) {
    case "Running":
      return `ralph:${iteration}/${max_iterations}`;

    case "Approved":
      return colorize("ralph:OK", GREEN);

    case "Rejected":
      return colorize("ralph:FAIL", RED);

    case "Cancelled":
      return colorize("ralph:--", DIM);

    default:
      return `ralph:${iteration}/${max_iterations}`;
  }
}
