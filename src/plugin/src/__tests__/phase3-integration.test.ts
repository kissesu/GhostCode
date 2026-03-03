/**
 * @file phase3-integration.test.ts
 * @description Phase 3 端到端集成测试
 *              测试 Keywords、Verification Report 两个 TS 模块之间的集成链路
 *              不依赖 Daemon IPC 的纯单元集成测试，测试 TS 侧各模块间的协作
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { describe, it, expect } from "vitest";
import { sanitizeForKeywordDetection } from "../keywords/sanitize";
import { detectMagicKeywords, resolveKeywordPriority } from "../keywords/parser";
import { formatReport } from "../verification/report";
import type { RunState } from "../verification/types";

// ============================================
// 测试辅助：构建 RunState 的工厂函数
// ============================================

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

// ============================================
// Phase 3 端到端集成测试套件
// ============================================

describe("Phase 3 端到端集成测试", () => {
  // ============================================
  // 1. keywords -> detection 完整流程
  // ============================================
  it("keywords -> detection 完整流程", () => {
    const input = "请启动 ralph 验证";
    const matches = detectMagicKeywords(input);

    const ralphMatch = matches.find((m) => m.type === "ralph");
    expect(ralphMatch).toBeDefined();
    expect(ralphMatch?.type).toBe("ralph");

    const top = resolveKeywordPriority(matches);
    expect(top).not.toBeNull();
    expect(top?.type).toBe("ralph");
  });

  // ============================================
  // 2. keywords 防误检集成
  // ============================================
  it("keywords 防误检集成 - 代码块中的 ralph 不应被检测到", () => {
    const input = "```\nralph\n```";

    const sanitized = sanitizeForKeywordDetection(input);
    expect(sanitized).not.toContain("ralph");

    const matches = detectMagicKeywords(input);
    expect(matches).toHaveLength(0);

    const top = resolveKeywordPriority(matches);
    expect(top).toBeNull();
  });

  // ============================================
  // 3. keywords 多关键词优先级
  // ============================================
  it("keywords 多关键词优先级 - ralph 优先级高于 team", () => {
    const input = "先 team 协作然后 ralph 验证";
    const matches = detectMagicKeywords(input);

    const teamMatch = matches.find((m) => m.type === "team");
    const ralphMatch = matches.find((m) => m.type === "ralph");
    expect(teamMatch).toBeDefined();
    expect(ralphMatch).toBeDefined();

    const top = resolveKeywordPriority(matches);
    expect(top).not.toBeNull();
    expect(top?.type).toBe("ralph");
  });

  // ============================================
  // 4. verification report 格式化（Approved 状态）
  // ============================================
  it("verification report 格式化 - Approved 状态 Markdown 包含所有检查项", () => {
    const state = buildApprovedRunState();

    const md = formatReport(state, "markdown");
    expect(md).toContain("approved");
    expect(md).toContain("Build");
    expect(md).toContain("Test");
    expect(md).toContain("Lint");
    expect(md).toContain("Functionality");
    expect(md).toContain("Architect");
    expect(md).toContain("Todo");
    expect(md).toContain("ErrorFree");

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
    expect(md).toContain("rejected");
    expect(md).toContain("编译失败");
  });

  // ============================================
  // 6. 完整管线集成：关键词 -> 验证报告
  // ============================================
  it("完整管线集成：关键词检测 -> 验证报告格式化", () => {
    // 第一步：关键词检测
    const userInput = "ralph";
    const matches = detectMagicKeywords(userInput);
    const topKeyword = resolveKeywordPriority(matches);

    expect(topKeyword).not.toBeNull();
    expect(topKeyword?.type).toBe("ralph");

    // 第二步：模拟创建 RunState
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

    // 第三步：格式化验证报告
    const report = formatReport(runState, "markdown");
    expect(report).toContain("approved");
    expect(report).toContain("Build");
  });
});
