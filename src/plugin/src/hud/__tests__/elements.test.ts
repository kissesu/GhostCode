/**
 * @file HUD 元素渲染测试
 * @description 对 HUD 各个元素渲染函数进行单元测试，覆盖各种状态和边界情况
 * @author Atlas.oi
 * @date 2026-03-03
 */
import { describe, it, expect } from "vitest";
import { renderRalph } from "../elements/ralph";
import { renderContext } from "../elements/context";
import { renderCost } from "../elements/cost";
import { renderModel } from "../elements/model";
import { renderAgent } from "../elements/agent";
import type { VerificationSummary, ContextPressure, CostSummary } from "../types";

// ============================================
// renderRalph 测试用例
// ============================================

describe("renderRalph", () => {
  it("有验证且状态为 Running 时，返回 ralph:iteration/max 格式", () => {
    const verification: VerificationSummary = {
      run_id: "run-1",
      group_id: "group-1",
      status: "Running",
      iteration: 3,
      max_iterations: 10,
      checks_passed: 2,
      checks_total: 5,
    };
    const result = renderRalph(verification);
    // 输出应包含 ralph:3/10（可能含 ANSI 颜色码）
    expect(result).toContain("ralph:3/10");
  });

  it("无验证时（null），返回空字符串", () => {
    const result = renderRalph(null);
    expect(result).toBe("");
  });

  it("状态为 Approved 时，返回含绿色标记的 ralph:OK", () => {
    const verification: VerificationSummary = {
      run_id: "run-2",
      group_id: "group-2",
      status: "Approved",
      iteration: 5,
      max_iterations: 10,
      checks_passed: 5,
      checks_total: 5,
    };
    const result = renderRalph(verification);
    expect(result).toContain("ralph:OK");
    // 绿色 ANSI 代码 \x1b[32m
    expect(result).toContain("\x1b[32m");
  });
});

// ============================================
// renderContext 测试用例
// ============================================

describe("renderContext", () => {
  it("percentage < 70 时返回绿色 ctx:45%", () => {
    const pressure: ContextPressure = {
      used_tokens: 45000,
      max_tokens: 100000,
      percentage: 45,
      level: "green",
    };
    const result = renderContext(pressure);
    expect(result).toContain("ctx:45%");
    // 绿色 ANSI 代码
    expect(result).toContain("\x1b[32m");
  });

  it("percentage 在 70-85 之间时返回黄色 ctx:72%", () => {
    const pressure: ContextPressure = {
      used_tokens: 72000,
      max_tokens: 100000,
      percentage: 72,
      level: "yellow",
    };
    const result = renderContext(pressure);
    expect(result).toContain("ctx:72%");
    // 黄色 ANSI 代码
    expect(result).toContain("\x1b[33m");
  });

  it("percentage > 85 时返回红色 ctx:92%", () => {
    const pressure: ContextPressure = {
      used_tokens: 92000,
      max_tokens: 100000,
      percentage: 92,
      level: "red",
    };
    const result = renderContext(pressure);
    expect(result).toContain("ctx:92%");
    // 红色 ANSI 代码
    expect(result).toContain("\x1b[31m");
  });
});

// ============================================
// renderCost 测试用例
// ============================================

describe("renderCost", () => {
  it("正确格式化美元金额 cost:$0.15", () => {
    const cost: CostSummary = {
      // 0.15 美元 = 0.15 * 100_000_000 = 15_000_000 micro-cents
      total_cost_micro: 15_000_000,
      total_prompt_tokens: 1000,
      total_completion_tokens: 500,
      request_count: 5,
    };
    const result = renderCost(cost);
    expect(result).toContain("cost:$0.15");
  });

  it("零成本时显示 cost:$0.00", () => {
    const cost: CostSummary = {
      total_cost_micro: 0,
      total_prompt_tokens: 0,
      total_completion_tokens: 0,
      request_count: 0,
    };
    const result = renderCost(cost);
    expect(result).toContain("cost:$0.00");
  });
});

// ============================================
// renderModel 测试用例
// ============================================

describe("renderModel", () => {
  it("显示传入的模型名 model:opus", () => {
    const result = renderModel("opus");
    expect(result).toContain("model:opus");
  });
});

// ============================================
// renderAgent 测试用例
// ============================================

describe("renderAgent", () => {
  it("显示活跃 Agent 数量 agents:2", () => {
    const result = renderAgent(2);
    expect(result).toContain("agents:2");
  });

  it("零 Agent 时显示 agents:0", () => {
    const result = renderAgent(0);
    expect(result).toContain("agents:0");
  });
});
