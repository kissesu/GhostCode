/**
 * @file ghostcode doctor CLI 命令
 * @description 聚合所有前端检查器，输出清晰的 PASS/FAIL 诊断列表。
 *              检查项涵盖：daemon 二进制、Node 版本、daemon 可达性、版本匹配、配置有效性。
 * @author Atlas.oi
 * @date 2026-03-04
 */

import type { CheckResult } from "../diagnostics/checkers";
import {
  checkBinaryPath,
  checkConfigValid,
  checkDaemonReachable,
  checkNodeVersion,
  checkVersionMatch,
} from "../diagnostics/checkers";

// ============================================
// 对外类型定义
// ============================================

/** doctor 命令的完整诊断报告 */
export interface DoctorReport {
  /** 所有检查项的结果列表 */
  checks: CheckResult[];
  /** 总体状态：任一检查 FAIL 则为 FAIL，否则为 PASS */
  overallStatus: "PASS" | "FAIL";
}

/**
 * 运行所有前端检查器并聚合结果
 *
 * 业务逻辑：
 * 1. 并发运行所有检查器以提高效率
 * 2. 汇总结果列表
 * 3. 若任一检查项为 FAIL，总体状态为 FAIL
 * 4. 所有检查项均 PASS 或 WARN 时，总体状态为 PASS
 *
 * @returns 诊断报告，包含所有检查结果和总体状态
 */
export async function runDoctor(): Promise<DoctorReport> {
  // ============================================
  // 并发运行所有检查器
  // 检查之间互相独立，使用 Promise.all 并发提升效率
  // ============================================
  const checks = await Promise.all([
    checkBinaryPath(),
    checkNodeVersion(),
    // checkDaemonReachable 依赖 addr.json，在未安装场景可能 FAIL
    checkDaemonReachable(),
    // checkVersionMatch 不传参数，会尝试读取 daemon-version 文件
    checkVersionMatch(),
    checkConfigValid(),
  ]);

  // ============================================
  // 聚合总体状态
  // 任一 FAIL 则总体为 FAIL，否则为 PASS（WARN 不影响总体）
  // ============================================
  const hasFail = checks.some((check) => check.status === "FAIL");
  const overallStatus: "PASS" | "FAIL" = hasFail ? "FAIL" : "PASS";

  return {
    checks,
    overallStatus,
  };
}

/**
 * 格式化诊断报告为终端友好的输出字符串
 *
 * 输出格式示例：
 * ```
 * GhostCode Doctor 诊断报告
 * ========================
 * [PASS] binary: ghostcoded 二进制存在于 /Users/xxx/.ghostcode/bin/ghostcoded
 * [FAIL] node-version: Node.js 版本 v18.0.0 低于最低要求，需要 >= 20
 *        建议: 请升级 Node.js 到 20 或更高版本，推荐使用 volta 管理版本
 * ========================
 * 总体状态: FAIL
 * ```
 *
 * @param report - 诊断报告对象
 * @returns 格式化后的字符串，适合直接输出到终端
 */
export function formatDoctorReport(report: DoctorReport): string {
  const lines: string[] = [];

  // ============================================
  // 标题
  // ============================================
  lines.push("GhostCode Doctor 诊断报告");
  lines.push("========================");

  // ============================================
  // 各检查项输出
  // 格式：[STATUS] name: message
  //       （如有建议）建议: suggestion
  // ============================================
  for (const check of report.checks) {
    // 状态标签：PASS/FAIL/WARN 对齐到 4 字符宽
    const statusLabel = check.status.padEnd(4);
    lines.push(`[${statusLabel}] ${check.name}: ${check.message}`);

    // FAIL 和 WARN 时显示修复建议
    if (check.suggestion !== undefined && (check.status === "FAIL" || check.status === "WARN")) {
      lines.push(`       建议: ${check.suggestion}`);
    }
  }

  // ============================================
  // 总体状态
  // ============================================
  lines.push("========================");
  lines.push(`总体状态: ${report.overallStatus}`);

  return lines.join("\n");
}
