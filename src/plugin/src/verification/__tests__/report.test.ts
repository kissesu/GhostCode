/**
 * @file report.test.ts
 * @description 验证报告格式化测试
 *              测试 formatReport 的 markdown/json 输出和 mapStatusToVerdict 映射
 *              TDD Red 阶段：测试先于实现
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { describe, it, expect } from "vitest";
import { formatReport, mapStatusToVerdict } from "../report.js";
import type { RunState } from "../types.js";

// ============================================
// 测试辅助：构造 RunState 快照
// ============================================

/**
 * 构建测试用 RunState，可覆盖任意字段
 */
function mockRunState(overrides: Partial<RunState> = {}): RunState {
  return {
    run_id: "run-001",
    group_id: "group-test",
    status: "Running",
    iteration: 0,
    max_iterations: 10,
    current_checks: [
      ["Build", "Pending"],
      ["Test", "Pending"],
      ["Lint", "Pending"],
    ],
    history: [],
    ...overrides,
  };
}

// ============================================
// 测试套件
// ============================================

describe("mapStatusToVerdict 状态映射", () => {
  it("Running 映射为 in_progress", () => {
    expect(mapStatusToVerdict("Running")).toBe("in_progress");
  });

  it("Approved 映射为 approved", () => {
    expect(mapStatusToVerdict("Approved")).toBe("approved");
  });

  it("Rejected 映射为 rejected", () => {
    expect(mapStatusToVerdict("Rejected")).toBe("rejected");
  });

  it("Cancelled 映射为 cancelled", () => {
    expect(mapStatusToVerdict("Cancelled")).toBe("cancelled");
  });

  it("未知状态映射为 in_progress（默认值）", () => {
    expect(mapStatusToVerdict("Unknown")).toBe("in_progress");
  });
});

describe("formatReport - Markdown 格式", () => {
  it("全部通过时输出 Approved 报告（含 [PASS] 标记）", () => {
    // Arrange: 所有检查通过，状态为 Approved
    const state = mockRunState({
      status: "Approved",
      iteration: 2,
      current_checks: [
        ["Build", "Passed"],
        ["Test", "Passed"],
        ["Lint", "Passed"],
      ],
    });

    // Act
    const report = formatReport(state, "markdown");

    // Assert: 包含标题、通过标记
    expect(report).toContain("Ralph 验证报告");
    expect(report).toContain("approved");
    expect(report).toContain("[PASS]");
    expect(report).not.toContain("[FAIL]");
  });

  it("有失败项时输出 Running 报告（含 [FAIL] 标记和失败原因）", () => {
    // Arrange: 部分检查失败，处于 Running 状态
    const state = mockRunState({
      status: "Running",
      iteration: 1,
      current_checks: [
        ["Build", "Passed"],
        ["Test", { Failed: "单元测试 3 个失败" }],
        ["Lint", "Pending"],
      ],
    });

    // Act
    const report = formatReport(state, "markdown");

    // Assert: 包含失败标记和失败描述
    expect(report).toContain("[FAIL]");
    expect(report).toContain("单元测试 3 个失败");
    expect(report).toContain("[PASS]");
    expect(report).toContain("[WAIT]");
    expect(report).toContain("in_progress");
  });

  it("空历史记录时报告正常输出（新启动的 run）", () => {
    // Arrange: 刚启动的运行，无历史记录
    const state = mockRunState({
      history: [],
      current_checks: [["Build", "Pending"]],
    });

    // Act
    const report = formatReport(state, "markdown");

    // Assert: 不包含历史记录章节
    expect(report).toContain("Ralph 验证报告");
    expect(report).not.toContain("历史记录");
    expect(report).toContain("[WAIT]");
  });

  it("有历史记录时输出历史记录章节", () => {
    // Arrange: 已完成两轮迭代
    const state = mockRunState({
      status: "Running",
      iteration: 2,
      history: [
        {
          checks: [{ kind: "Build", status: "Passed" }],
          failure_reasons: ["Lint 不通过"],
        },
        {
          checks: [{ kind: "Build", status: "Passed" }],
          failure_reasons: [],
        },
      ],
    });

    // Act
    const report = formatReport(state, "markdown");

    // Assert: 包含历史记录章节和失败数量
    expect(report).toContain("历史记录");
    expect(report).toContain("第 1 轮");
    expect(report).toContain("第 2 轮");
    expect(report).toContain("1 项失败");
    expect(report).toContain("全部通过");
  });
});

describe("formatReport - JSON 格式", () => {
  it("输出结构正确的 JSON 报告", () => {
    // Arrange
    const state = mockRunState({
      status: "Approved",
      iteration: 3,
      max_iterations: 10,
      current_checks: [
        ["Build", "Passed"],
        ["Test", "Passed"],
      ],
      history: [
        { checks: [], failure_reasons: [] },
        { checks: [], failure_reasons: [] },
        { checks: [], failure_reasons: [] },
      ],
    });

    // Act
    const report = formatReport(state, "json");

    // Assert: 解析 JSON 并验证结构
    const parsed = JSON.parse(report) as Record<string, unknown>;
    expect(parsed["verdict"]).toBe("approved");
    expect(parsed["iteration"]).toBe(3);
    expect(parsed["max_iterations"]).toBe(10);
    expect(parsed["history_count"]).toBe(3);
    expect(Array.isArray(parsed["checks"])).toBe(true);

    const checks = parsed["checks"] as Array<{ kind: string; status: string }>;
    expect(checks).toHaveLength(2);
    expect(checks[0]).toMatchObject({ kind: "Build", status: "通过" });
    expect(checks[1]).toMatchObject({ kind: "Test", status: "通过" });
  });

  it("JSON 格式默认不传 format 参数时输出 markdown", () => {
    // Arrange
    const state = mockRunState();

    // Act: 不传 format 参数
    const report = formatReport(state);

    // Assert: 默认为 markdown 格式
    expect(report).toContain("Ralph 验证报告");
    expect(() => JSON.parse(report)).toThrow();
  });
});
