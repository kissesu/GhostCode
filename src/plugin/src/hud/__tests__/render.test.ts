/**
 * @file HUD 状态栏渲染测试
 * @description 对 renderStatusline 函数进行集成测试，覆盖完整组合、空快照和无验证等场景
 * @author Atlas.oi
 * @date 2026-03-03
 */
import { describe, it, expect } from "vitest";
import { renderStatusline } from "../render";
import type { HudSnapshot } from "../types";

// ============================================
// 测试辅助数据
// ============================================

/** 完整的 HudSnapshot 测试数据 */
const fullSnapshot: HudSnapshot = {
  verification: {
    run_id: "run-1",
    group_id: "group-1",
    status: "Running",
    iteration: 3,
    max_iterations: 10,
    checks_passed: 2,
    checks_total: 5,
  },
  cost: {
    // 0.15 美元 = 15_000_000 micro-cents
    total_cost_micro: 15_000_000,
    total_prompt_tokens: 1000,
    total_completion_tokens: 500,
    request_count: 5,
  },
  context_pressure: {
    used_tokens: 72000,
    max_tokens: 100000,
    percentage: 72,
    level: "yellow",
  },
  active_agents: 2,
};

/** 无验证的 HudSnapshot 测试数据 */
const noVerificationSnapshot: HudSnapshot = {
  verification: null,
  cost: {
    total_cost_micro: 15_000_000,
    total_prompt_tokens: 1000,
    total_completion_tokens: 500,
    request_count: 5,
  },
  context_pressure: {
    used_tokens: 72000,
    max_tokens: 100000,
    percentage: 72,
    level: "yellow",
  },
  active_agents: 2,
};

/** 空/零值的 HudSnapshot 测试数据，用于测试边界安全性 */
const emptySnapshot: HudSnapshot = {
  verification: null,
  cost: {
    total_cost_micro: 0,
    total_prompt_tokens: 0,
    total_completion_tokens: 0,
    request_count: 0,
  },
  context_pressure: {
    used_tokens: 0,
    max_tokens: 100000,
    percentage: 0,
    level: "green",
  },
  active_agents: 0,
};

// ============================================
// renderStatusline 测试用例
// ============================================

describe("renderStatusline", () => {
  it("完整快照：组合所有元素，格式为 [GC] ralph:3/10 | ctx:72% | cost:$0.15 | agents:2", () => {
    const result = renderStatusline(fullSnapshot, { modelName: "opus", useColors: false });
    expect(result).toContain("[GC]");
    expect(result).toContain("ralph:3/10");
    expect(result).toContain("ctx:72%");
    expect(result).toContain("cost:$0.15");
    expect(result).toContain("agents:2");
    // 各元素之间用 " | " 分隔
    expect(result).toContain(" | ");
  });

  it("空快照（零值）：安全渲染，不会 crash", () => {
    // 应该正常返回，不抛出异常
    expect(() => renderStatusline(emptySnapshot)).not.toThrow();
    const result = renderStatusline(emptySnapshot);
    expect(result).toContain("[GC]");
  });

  it("无验证时省略 ralph 段", () => {
    const result = renderStatusline(noVerificationSnapshot, { useColors: false });
    // ralph 字段不应该出现（无验证时为空字符串，被过滤掉）
    expect(result).not.toContain("ralph:");
    // 其他字段仍然存在
    expect(result).toContain("ctx:");
    expect(result).toContain("cost:");
    expect(result).toContain("agents:");
  });
});
