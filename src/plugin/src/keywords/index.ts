/**
 * @file index.ts
 * @description Magic Keywords 模块统一导出入口
 * 提供关键词检测、优先级解析、文本清理和状态管理功能
 * @author Atlas.oi
 * @date 2026-03-03
 */

// 类型定义
export type { KeywordType, KeywordMatch, KeywordState } from "./types.js";

// 文本清理
export { sanitizeForKeywordDetection } from "./sanitize.js";

// 关键词解析
export {
  KEYWORD_PATTERNS,
  detectMagicKeywords,
  resolveKeywordPriority,
} from "./parser.js";

// 状态管理
export { readKeywordState, writeKeywordState } from "./state.js";
