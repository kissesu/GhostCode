/**
 * @file report.ts
 * @description 验证报告格式化输出
 *              将 RunState 转换为人类可读的 Markdown 或 JSON 报告
 *              参考: oh-my-claudecode/src/skills/ralph.ts - Ralph 验证循环输出
 * @author Atlas.oi
 * @date 2026-03-03
 */

import type { RunState, CheckStatus, Verdict, ReportFormat } from "./types.js";

// ============================================
// 状态映射
// ============================================

/**
 * 将 Rust RunStatus 字符串映射为语义化 Verdict
 *
 * 业务逻辑说明：
 * - Approved → approved（验证通过）
 * - Rejected → rejected（验证失败，超过最大迭代次数）
 * - Cancelled → cancelled（用户主动取消）
 * - Running 及其他 → in_progress（仍在运行中）
 *
 * @param status - RunStatus 字符串值
 * @returns 语义化的 Verdict 判定结果
 */
export function mapStatusToVerdict(status: string): Verdict {
  switch (status) {
    case "Approved": return "approved";
    case "Rejected": return "rejected";
    case "Cancelled": return "cancelled";
    default: return "in_progress";
  }
}

// ============================================
// 内部辅助函数
// ============================================

/**
 * 将 CheckStatus 格式化为中文可读字符串
 *
 * @param status - 单项检查状态
 * @returns 中文描述字符串
 */
function formatCheckStatus(status: CheckStatus): string {
  if (status === "Pending") return "待检查";
  if (status === "Passed") return "通过";
  if (typeof status === "object" && "Failed" in status) {
    return `失败: ${status.Failed}`;
  }
  return "未知";
}

/**
 * 根据检查状态返回对应图标标记
 *
 * @param status - 单项检查状态
 * @returns ASCII 图标标记（不使用 emoji，遵循项目规范）
 */
function getStatusIcon(status: CheckStatus): string {
  if (status === "Passed") return "[PASS]";
  if (status === "Pending") return "[WAIT]";
  if (typeof status === "object" && "Failed" in status) return "[FAIL]";
  return "[????]";
}

// ============================================
// 公共 API
// ============================================

/**
 * 格式化验证报告
 *
 * 业务逻辑说明：
 * 1. 根据 format 参数选择 markdown 或 json 输出格式
 * 2. Markdown 格式：输出人类可读的表格式报告，含历史记录
 * 3. JSON 格式：输出结构化数据，适合程序消费
 * 4. 默认为 markdown 格式（适合直接展示给用户）
 *
 * @param state - 当前 RunState 快照
 * @param format - 输出格式，默认为 "markdown"
 * @returns 格式化后的报告字符串
 */
export function formatReport(state: RunState, format: ReportFormat = "markdown"): string {
  if (format === "json") {
    return formatJsonReport(state);
  }
  return formatMarkdownReport(state);
}

/**
 * 格式化 JSON 结构报告
 *
 * @param state - RunState 快照
 * @returns 格式化的 JSON 字符串
 */
function formatJsonReport(state: RunState): string {
  return JSON.stringify(
    {
      verdict: mapStatusToVerdict(state.status as string),
      iteration: state.iteration,
      max_iterations: state.max_iterations,
      checks: state.current_checks.map(([kind, status]) => ({
        kind,
        status: formatCheckStatus(status),
      })),
      history_count: state.history.length,
    },
    null,
    2
  );
}

/**
 * 格式化 Markdown 表格报告
 *
 * @param state - RunState 快照
 * @returns Markdown 格式的报告字符串
 */
function formatMarkdownReport(state: RunState): string {
  const verdict = mapStatusToVerdict(state.status as string);
  const lines: string[] = [];

  // ============================================
  // 报告标题与摘要信息
  // ============================================
  lines.push(`## Ralph 验证报告`);
  lines.push(``);
  lines.push(
    `**状态**: ${verdict} | **迭代**: ${state.iteration + 1}/${state.max_iterations}`
  );
  lines.push(``);

  // ============================================
  // 当前检查状态表格
  // ============================================
  lines.push(`| 检查项 | 状态 |`);
  lines.push(`|--------|------|`);

  for (const [kind, status] of state.current_checks) {
    const icon = getStatusIcon(status);
    lines.push(`| ${kind} | ${icon} ${formatCheckStatus(status)} |`);
  }

  // ============================================
  // 历史记录章节（仅当有历史记录时输出）
  // ============================================
  if (state.history.length > 0) {
    lines.push(``);
    lines.push(`### 历史记录`);
    for (let i = 0; i < state.history.length; i++) {
      const iter = state.history[i];
      if (iter === undefined) continue;
      const failCount = iter.failure_reasons.length;
      const summary = failCount === 0 ? "全部通过" : `${failCount} 项失败`;
      lines.push(`- 第 ${i + 1} 轮: ${summary}`);
    }
  }

  return lines.join("\n");
}
