/**
 * @file keyword-integration.test.ts
 * @description Keywords 集成到 UserPromptSubmit Hook 的集成测试
 *              测试关键词检测 → 状态写入 → additionalContext 返回的完整链路
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { describe, it, expect, vi, beforeEach } from "vitest";

// ============================================
// Mock daemon.ts 模块，隔离外部依赖
// ============================================
vi.mock("../../daemon.js", () => ({
  ensureDaemon: vi.fn(),
  stopDaemon: vi.fn(),
  startHeartbeat: vi.fn(),
}));

// ============================================
// Mock keywords/state.ts 的文件系统操作
// 避免集成测试依赖真实文件系统
// ============================================
vi.mock("../../keywords/state.js", () => ({
  writeKeywordState: vi.fn().mockResolvedValue(undefined),
  readKeywordState: vi.fn().mockResolvedValue({
    active: null,
    activatedAt: null,
    prompt: null,
  }),
}));

describe("userPromptSubmitHandler - Keywords 集成", () => {
  beforeEach(() => {
    // 清除 mock 调用记录，避免测试间互相污染
    vi.clearAllMocks();
    // 重置模块，确保每次测试获取新的 handler 实例
    vi.resetModules();
  });

  // ============================================
  // 测试 1：检测到 ralph 时返回 additionalContext
  // ralph 是关键词之一，应触发激活并返回上下文说明
  // ============================================
  it("检测到 ralph 时返回 additionalContext", async () => {
    const { userPromptSubmitHandler } = await import("../handlers.js");

    const result = await userPromptSubmitHandler({
      prompt: "ralph check my code",
    });

    // 应返回包含 additionalContext 的对象，而非 undefined
    expect(result).not.toBeUndefined();
    expect(result).toHaveProperty("additionalContext");
    expect((result as { additionalContext: string }).additionalContext).toContain(
      "Ralph"
    );
  });

  // ============================================
  // 测试 2：检测到 autopilot 时返回 additionalContext
  // autopilot 变体包括 "auto pilot"、"auto-pilot"、"full auto"
  // ============================================
  it("检测到 autopilot 时返回 additionalContext", async () => {
    const { userPromptSubmitHandler } = await import("../handlers.js");

    const result = await userPromptSubmitHandler({
      prompt: "auto pilot mode",
    });

    expect(result).not.toBeUndefined();
    expect(result).toHaveProperty("additionalContext");
    expect((result as { additionalContext: string }).additionalContext).toContain(
      "Autopilot"
    );
  });

  // ============================================
  // 测试 3：无关键词时返回 undefined
  // 普通 prompt 不应触发任何模式激活
  // ============================================
  it("无关键词时返回 undefined", async () => {
    const { userPromptSubmitHandler } = await import("../handlers.js");

    const result = await userPromptSubmitHandler({
      prompt: "please fix the bug",
    });

    expect(result).toBeUndefined();
  });

  // ============================================
  // 测试 4：检测到 cancel 时清除激活状态
  // cancel 是最高优先级关键词，用于清除已激活的模式
  // ============================================
  it("检测到 cancel 时清除激活状态", async () => {
    // 模拟已有 ralph 激活状态
    const { writeKeywordState } = await import("../../keywords/state.js");
    const mockWriteKeywordState = writeKeywordState as ReturnType<typeof vi.fn>;

    const { userPromptSubmitHandler } = await import("../handlers.js");

    const result = await userPromptSubmitHandler({
      prompt: "cancel",
    });

    // 应返回 additionalContext（告知用户已取消）
    expect(result).not.toBeUndefined();
    expect(result).toHaveProperty("additionalContext");

    // 应调用 writeKeywordState 清除状态（active 为 null）
    expect(mockWriteKeywordState).toHaveBeenCalledWith(
      expect.any(String),
      expect.objectContaining({
        active: null,
        activatedAt: null,
        prompt: null,
      })
    );
  });

  // ============================================
  // 测试 5：代码块中的关键词不触发
  // sanitize 预处理会移除代码块内容，防止误触发
  // ============================================
  it("代码块中的关键词不触发", async () => {
    const { userPromptSubmitHandler } = await import("../handlers.js");

    const result = await userPromptSubmitHandler({
      prompt: "```\nralph\n```",
    });

    // 代码块内的 ralph 不应触发激活
    expect(result).toBeUndefined();
  });

  // ============================================
  // 测试 6：多关键词时选择最高优先级
  // 优先级：cancel(1) > ralph(2) > autopilot(3) > team(4) > ultrawork(5)
  // 同时包含 ralph、team、autopilot 时，ralph 应胜出
  // ============================================
  it("多关键词时选择最高优先级（ralph 优先于 autopilot 和 team）", async () => {
    const { userPromptSubmitHandler } = await import("../handlers.js");

    const result = await userPromptSubmitHandler({
      prompt: "ralph team autopilot",
    });

    expect(result).not.toBeUndefined();
    expect(result).toHaveProperty("additionalContext");
    // ralph 优先级最高（排除 cancel），应选择 ralph 的上下文
    expect((result as { additionalContext: string }).additionalContext).toContain(
      "Ralph"
    );
  });
});
