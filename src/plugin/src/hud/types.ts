/**
 * @file HUD 类型定义
 * @description 定义 HudSnapshot 接口及相关类型，与 Rust Daemon 侧 hud_snapshot op 返回的 JSON 结构对齐
 * @author Atlas.oi
 * @date 2026-03-03
 */

// ============================================
// 颜色级别类型
// 用于上下文压力等级的颜色标记
// ============================================

/** 颜色级别：绿/黄/红，对应安全/警告/危险三个状态 */
export type ColorLevel = "green" | "yellow" | "red";

// ============================================
// HUD 渲染元素类型
// 渲染后的字符串片段，包含可选的 ANSI 颜色码
// ============================================

/** HUD 元素：渲染完成的可显示字符串片段 */
export type HudElement = string;

// ============================================
// 验证状态接口
// 对应 Rust 侧 VerificationSummary 结构体
// ============================================

/** 验证运行状态枚举值 */
export type VerificationStatus = "Running" | "Approved" | "Rejected" | "Cancelled";

/** 验证状态摘要，无验证运行时整体为 null */
export interface VerificationSummary {
  /** 验证运行 ID */
  run_id: string;
  /** 分组 ID */
  group_id: string;
  /** 当前状态 */
  status: VerificationStatus;
  /** 当前迭代次数 */
  iteration: number;
  /** 最大允许迭代次数 */
  max_iterations: number;
  /** 已通过的检查项数量 */
  checks_passed: number;
  /** 总检查项数量 */
  checks_total: number;
}

// ============================================
// 成本汇总接口
// 对应 Rust 侧 CostSummary 结构体
// ============================================

/** 成本汇总数据 */
export interface CostSummary {
  /** 总成本（单位：micro-cents，1 美元 = 100_000_000） */
  total_cost_micro: number;
  /** 总 Prompt Token 数量 */
  total_prompt_tokens: number;
  /** 总 Completion Token 数量 */
  total_completion_tokens: number;
  /** 总请求次数 */
  request_count: number;
}

// ============================================
// 上下文压力接口
// 对应 Rust 侧 ContextPressure 结构体
// ============================================

/** 上下文 Token 压力状态 */
export interface ContextPressure {
  /** 已使用 Token 数量 */
  used_tokens: number;
  /** 最大允许 Token 数量 */
  max_tokens: number;
  /** 使用百分比（0.0 - 100.0） */
  percentage: number;
  /** 压力级别 */
  level: ColorLevel;
}

// ============================================
// HudSnapshot 主接口
// Daemon hud_snapshot op 的完整返回结构
// ============================================

/** HUD 状态快照 - Daemon hud_snapshot op 的完整返回结构 */
export interface HudSnapshot {
  /** 验证状态摘要，无验证运行时为 null */
  verification: VerificationSummary | null;
  /** 成本汇总 */
  cost: CostSummary;
  /** 上下文压力 */
  context_pressure: ContextPressure;
  /** 活跃 Agent 数量，null 表示无法获取（RwLock 被占用） */
  active_agents: number | null;
}

// ============================================
// renderStatusline 的可选配置
// ============================================

/** renderStatusline 函数的可选配置项 */
export interface StatuslineOptions {
  /** 模型名称（不在 snapshot 中，从外部传入，默认 "unknown"） */
  modelName?: string;
  /** 是否显示 ANSI 颜色码（默认 true） */
  useColors?: boolean;
}
