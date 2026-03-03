/**
 * @file types.ts
 * @description Magic Keywords 模块的类型定义
 * 定义关键词类型、匹配结果和状态结构
 * @author Atlas.oi
 * @date 2026-03-03
 */

/**
 * 支持的 Magic Keyword 类型
 * 优先级从高到低：cancel(1) > ralph(2) > autopilot(3) > team(4) > ultrawork(5)
 */
export type KeywordType = "cancel" | "ralph" | "autopilot" | "team" | "ultrawork";

/**
 * 关键词匹配结果
 * 包含匹配到的关键词类型、优先级和原始匹配字符串
 */
export interface KeywordMatch {
  /** 关键词类型 */
  type: KeywordType;
  /** 优先级数值，数字越小优先级越高 */
  priority: number;
  /** 原始匹配字符串 */
  match: string;
}

/**
 * 关键词激活状态
 * 持久化到 .ghostcode/state/keywords.json 中
 */
export interface KeywordState {
  /** 当前激活的关键词，null 表示无激活状态 */
  active: KeywordType | null;
  /** 激活时间 ISO 8601 格式，null 表示未激活 */
  activatedAt: string | null;
  /** 触发激活的原始提示词，null 表示未激活 */
  prompt: string | null;
}
