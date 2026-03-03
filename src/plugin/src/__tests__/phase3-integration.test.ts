/**
 * @file phase3-integration.test.ts
 * @description Phase 3 端到端集成测试
 *              测试 Keywords、Verification Report、HUD 三个 TS 模块之间的集成链路
 *              这些是不依赖 Daemon IPC 的纯单元集成测试，测试 TS 侧各模块间的协作
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { describe, it, expect } from "vitest";
import { sanitizeForKeywordDetection } from "../keywords/sanitize";
import { detectMagicKeywords, resolveKeywordPriority } from "../keywords/parser";
import { formatReport } from "../verification/report";
import type { RunState } from "../verification/types";
import { renderStatusline } from "../hud/render";
import { renderContext } from "../hud/elements/context";
import { renderCost } from "../hud/elements/cost";
import type {
  HudSnapshot,
  VerificationSummary,
  CostSummary,
  ContextPressure,
} from "../hud/types";

// ============================================
// 测试辅助：构建 RunState 的工厂函数
// 避免在每个测试中重复构造冗长的结构体
// ============================================

/**
 * 构建所有检查项均通过的 RunState（Approved 状态）
 */
function buildApprovedRunState(): RunState {
  return {
    run_id: "run-001",
    group_id: "group-001",
    status: "Approved",
    iteration: 0,
    max_iterations: 3,
    current_checks: [
      ["Build", "Passed"],
      ["Test", "Passed"],
      ["Lint", "Passed"],
      ["Functionality", "Passed"],
      ["Architect", "Passed"],
      ["Todo", "Passed"],
      ["ErrorFree", "Passed"],
    ],
    history: [],
  };
}

/**
 * 构建有失败项的 RunState（Rejected 状态）
 */
function buildRejectedRunState(): RunState {
  return {
    run_id: "run-002",
    group_id: "group-001",
    status: "Rejected",
    iteration: 2,
    max_iterations: 3,
    current_checks: [
      ["Build", "Passed"],
      ["Test", { Failed: "编译失败" }],
    ],
    history: [],
  };
}

/**
 * 构建基础 CostSummary
 */
function buildCostSummary(totalCostMicro: number): CostSummary {
  return {
    total_cost_micro: totalCostMicro,
    total_prompt_tokens: 1000,
    total_completion_tokens: 500,
    request_count: 5,
  };
}

/**
 * 构建 ContextPressure
 */
function buildContextPressure(
  percentage: number,
  level: ContextPressure["level"]
): ContextPressure {
  return {
    used_tokens: Math.round(percentage * 2000),
    max_tokens: 200000,
    percentage,
    level,
  };
}

/**
 * 构建完整的 HudSnapshot（含验证状态）
 */
function buildFullHudSnapshot(): HudSnapshot {
  const verification: VerificationSummary = {
    run_id: "run-001",
    group_id: "group-001",
    status: "Running",
    iteration: 3,
    max_iterations: 10,
    checks_passed: 5,
    checks_total: 7,
  };

  return {
    verification,
    cost: buildCostSummary(15_000_000),
    context_pressure: buildContextPressure(72, "yellow"),
    active_agents: 2,
  };
}

// ============================================
// Phase 3 端到端集成测试套件
// ============================================

describe("Phase 3 端到端集成测试", () => {
  // ============================================
  // 1. keywords -> detection 完整流程
  // ============================================
  it("keywords -> detection 完整流程", () => {
    // 输入包含 ralph 关键词的自然语言
    const input = "请启动 ralph 验证";

    // detectMagicKeywords 内部已包含 sanitize 步骤
    const matches = detectMagicKeywords(input);

    // 应至少检测到 ralph
    const ralphMatch = matches.find((m) => m.type === "ralph");
    expect(ralphMatch).toBeDefined();
    expect(ralphMatch?.type).toBe("ralph");

    // 解析最高优先级关键词
    const top = resolveKeywordPriority(matches);
    expect(top).not.toBeNull();
    expect(top?.type).toBe("ralph");
  });

  // ============================================
  // 2. keywords 防误检集成
  // ============================================
  it("keywords 防误检集成 - 代码块中的 ralph 不应被检测到", () => {
    // 输入：ralph 在代码块内，不应被检测为关键词
    const input = "```\nralph\n```";

    // sanitize 先清除代码块内容
    const sanitized = sanitizeForKeywordDetection(input);
    expect(sanitized).not.toContain("ralph");

    // detectMagicKeywords 内部会 sanitize，因此不会匹配到 ralph
    const matches = detectMagicKeywords(input);
    expect(matches).toHaveLength(0);

    // 优先级解析结果为 null
    const top = resolveKeywordPriority(matches);
    expect(top).toBeNull();
  });

  // ============================================
  // 3. keywords 多关键词优先级
  // ============================================
  it("keywords 多关键词优先级 - ralph 优先级高于 team", () => {
    // 输入同时包含 team 和 ralph
    const input = "先 team 协作然后 ralph 验证";

    const matches = detectMagicKeywords(input);

    // 应同时检测到 team 和 ralph
    const teamMatch = matches.find((m) => m.type === "team");
    const ralphMatch = matches.find((m) => m.type === "ralph");
    expect(teamMatch).toBeDefined();
    expect(ralphMatch).toBeDefined();

    // ralph 的优先级数值（2）小于 team（4），resolveKeywordPriority 应选择 ralph
    const top = resolveKeywordPriority(matches);
    expect(top).not.toBeNull();
    expect(top?.type).toBe("ralph");
  });

  // ============================================
  // 4. verification report 格式化（Approved 状态）
  // ============================================
  it("verification report 格式化 - Approved 状态 Markdown 包含所有检查项", () => {
    const state = buildApprovedRunState();

    // Markdown 格式
    // formatMarkdownReport 内部调用 mapStatusToVerdict，将 "Approved" 映射为 "approved"
    const md = formatReport(state, "markdown");
    expect(md).toContain("approved");
    // 验证所有 7 种检查类型都出现在报告中
    expect(md).toContain("Build");
    expect(md).toContain("Test");
    expect(md).toContain("Lint");
    expect(md).toContain("Functionality");
    expect(md).toContain("Architect");
    expect(md).toContain("Todo");
    expect(md).toContain("ErrorFree");

    // JSON 格式
    const json = formatReport(state, "json");
    expect(() => JSON.parse(json)).not.toThrow();
    const parsed = JSON.parse(json) as { verdict: string };
    expect(parsed.verdict).toBe("approved");
  });

  // ============================================
  // 5. verification report 失败状态
  // ============================================
  it("verification report 失败状态 - Markdown 包含失败原因", () => {
    const state = buildRejectedRunState();

    const md = formatReport(state, "markdown");

    // 输出应包含 Rejected 对应的 verdict 字符串
    expect(md).toContain("rejected");
    // 输出应包含失败原因
    expect(md).toContain("编译失败");
  });

  // ============================================
  // 6. HUD 渲染完整快照
  // ============================================
  it("HUD 渲染完整快照 - 包含所有关键字段", () => {
    const snapshot = buildFullHudSnapshot();

    const output = renderStatusline(snapshot, {
      modelName: "opus",
      useColors: false,
    });

    // 验证必要字段出现在输出中
    expect(output).toContain("[GC]");
    // ralph 元素（Running 状态显示迭代进度）
    expect(output).toContain("ralph:");
    // context 压力（72%）
    expect(output).toContain("ctx:72%");
    // cost 成本
    expect(output).toContain("cost:");
    // model 名称
    expect(output).toContain("model:opus");
    // agents 数量
    expect(output).toContain("agents:");
  });

  // ============================================
  // 7. HUD 渲染空状态
  // ============================================
  it("HUD 渲染空状态 - 不 crash，无验证时不显示 ralph", () => {
    const snapshot: HudSnapshot = {
      verification: null,
      cost: buildCostSummary(0),
      context_pressure: buildContextPressure(0, "green"),
      active_agents: 0,
    };

    // 不应抛出异常
    let output: string;
    expect(() => {
      output = renderStatusline(snapshot, { useColors: false });
    }).not.toThrow();

    output = renderStatusline(snapshot, { useColors: false });

    // 必须包含 [GC] 前缀
    expect(output).toContain("[GC]");
    // 无验证时不应包含 ralph: 元素
    expect(output).not.toContain("ralph:");
  });

  // ============================================
  // 8. HUD 上下文压力阈值渲染
  // ============================================
  it("HUD 上下文压力阈值渲染 - 三个级别均正确显示百分比", () => {
    // green 级别（45%）
    const green = renderContext(buildContextPressure(45, "green"));
    // 去除 ANSI 转义码后验证内容
    const greenPlain = green.replace(/\x1b\[[0-9;]*m/g, "");
    expect(greenPlain).toContain("ctx:45%");

    // yellow 级别（72%）
    const yellow = renderContext(buildContextPressure(72, "yellow"));
    const yellowPlain = yellow.replace(/\x1b\[[0-9;]*m/g, "");
    expect(yellowPlain).toContain("ctx:72%");

    // red 级别（92%）
    const red = renderContext(buildContextPressure(92, "red"));
    const redPlain = red.replace(/\x1b\[[0-9;]*m/g, "");
    expect(redPlain).toContain("ctx:92%");
  });

  // ============================================
  // 9. HUD 成本显示格式
  // ============================================
  it("HUD 成本显示格式 - micro-cents 正确转换为美元", () => {
    // $0.15 = 15_000_000 micro-cents
    const cost15c = renderCost(buildCostSummary(15_000_000));
    const cost15cPlain = cost15c.replace(/\x1b\[[0-9;]*m/g, "");
    expect(cost15cPlain).toContain("cost:$0.15");

    // $0.00 = 0 micro-cents
    const cost0 = renderCost(buildCostSummary(0));
    const cost0Plain = cost0.replace(/\x1b\[[0-9;]*m/g, "");
    expect(cost0Plain).toContain("cost:$0.00");

    // $1.00 = 100_000_000 micro-cents
    const cost1 = renderCost(buildCostSummary(100_000_000));
    const cost1Plain = cost1.replace(/\x1b\[[0-9;]*m/g, "");
    expect(cost1Plain).toContain("cost:$1.00");
  });

  // ============================================
  // 10. 完整管线集成：关键词 -> 验证 -> HUD
  // ============================================
  it("完整管线集成：关键词检测 -> 验证报告 -> HUD 渲染", () => {
    // ============================================
    // 第一步：关键词检测
    // 用户输入包含 ralph，触发验证流程
    // ============================================
    const userInput = "ralph";
    const matches = detectMagicKeywords(userInput);
    const topKeyword = resolveKeywordPriority(matches);

    expect(topKeyword).not.toBeNull();
    expect(topKeyword?.type).toBe("ralph");

    // ============================================
    // 第二步：模拟创建 RunState（手动构造，不需要 IPC）
    // 基于关键词触发的验证运行
    // ============================================
    const runState: RunState = {
      run_id: "run-pipeline-001",
      group_id: "group-pipeline",
      status: "Approved",
      iteration: 1,
      max_iterations: 3,
      current_checks: [
        ["Build", "Passed"],
        ["Test", "Passed"],
        ["Lint", "Passed"],
      ],
      history: [
        {
          checks: [
            { kind: "Build", status: "Passed" },
            { kind: "Test", status: "Passed" },
            { kind: "Lint", status: "Passed" },
          ],
          failure_reasons: [],
        },
      ],
    };

    // ============================================
    // 第三步：格式化验证报告
    // ============================================
    const report = formatReport(runState, "markdown");
    expect(report).toContain("approved");
    expect(report).toContain("Build");

    // ============================================
    // 第四步：构造 HudSnapshot（从 RunState 提取数据）
    // 将 RunState 映射为 HudSnapshot 中的 VerificationSummary
    // ============================================
    const verificationSummary: VerificationSummary = {
      run_id: runState.run_id,
      group_id: runState.group_id,
      status: runState.status,
      iteration: runState.iteration,
      max_iterations: runState.max_iterations,
      checks_passed: runState.current_checks.filter(
        ([, s]) => s === "Passed"
      ).length,
      checks_total: runState.current_checks.length,
    };

    const hudSnapshot: HudSnapshot = {
      verification: verificationSummary,
      cost: buildCostSummary(5_000_000),
      context_pressure: buildContextPressure(55, "green"),
      active_agents: 1,
    };

    // ============================================
    // 第五步：渲染 HUD 状态栏
    // 验证完整管线输出包含关键信息
    // ============================================
    const statusline = renderStatusline(hudSnapshot, {
      modelName: "sonnet",
      useColors: false,
    });

    // ralph 状态（Approved 时显示 ralph:OK）
    expect(statusline).toContain("ralph:");
    // 成本信息
    expect(statusline).toContain("cost:");
    // HUD 标识
    expect(statusline).toContain("[GC]");
  });
});
