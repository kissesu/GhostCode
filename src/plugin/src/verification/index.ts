/**
 * @file index.ts
 * @description 验证客户端模块统一导出
 *              提供完整的 TS 验证生命周期 API 和类型定义
 * @author Atlas.oi
 * @date 2026-03-03
 */

// ============================================
// 类型导出（与 Rust 侧 RunState 对齐）
// ============================================

export type {
  VerificationCheckKind,
  CheckStatus,
  RunStatus,
  VerificationCheckResult,
  VerificationIteration,
  RunState,
  ReportFormat,
  Verdict,
} from "./types.js";

// ============================================
// 客户端 API 导出（IPC 调用封装）
// ============================================

export {
  startVerification,
  getVerificationStatus,
  cancelVerification,
  VerificationError,
} from "./client.js";

// ============================================
// 报告格式化导出
// ============================================

export { formatReport, mapStatusToVerdict } from "./report.js";
