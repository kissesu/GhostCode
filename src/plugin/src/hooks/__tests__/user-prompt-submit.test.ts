/**
 * @file user-prompt-submit.test.ts
 * @description UserPromptSubmit Hook 处理器单元测试（TDD Red 阶段）
 *              测试 userPromptSubmitHandler 的注册和触发行为。
 *              验证空壳行为：handler 可注册、可触发、返回 undefined。
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { describe, it, expect, vi, beforeEach } from "vitest";

// ============================================
// Mock daemon.ts 模块，隔离外部依赖
// ============================================
vi.mock("../daemon.js", () => ({
  ensureDaemon: vi.fn(),
  stopDaemon: vi.fn(),
  startHeartbeat: vi.fn(),
}));

import { clearHooks, getHooks } from "../registry.js";

// ============================================
// 测试套件：userPromptSubmitHandler
// ============================================

describe("userPromptSubmitHandler", () => {
  beforeEach(async () => {
    // 清除 mock 调用记录，避免测试间互相污染
    vi.clearAllMocks();
    // 清除已注册的 Hook，保持测试隔离
    clearHooks();
    // 重置模块状态
    vi.resetModules();
  });

  it("handler 应能成功注册到 registry", async () => {
    const [
      { initializeHooks },
      { getHooks: getHooksFresh, clearHooks: clearHooksFresh },
    ] = await Promise.all([
      import("../handlers.js"),
      import("../registry.js"),
    ]);

    // 清理状态后注册
    clearHooksFresh();
    initializeHooks();

    // UserPromptSubmit 类型应有处理器注册
    const handlers = getHooksFresh("UserPromptSubmit");
    expect(handlers.length).toBeGreaterThan(0);
  });

  it("注册后 handler 应能被触发且不报错", async () => {
    vi.resetModules();
    const { userPromptSubmitHandler } = await import("../handlers.js");

    // 调用 handler，不应抛出任何错误
    await expect(
      userPromptSubmitHandler({ session_id: "test-session" }),
    ).resolves.not.toThrow();
  });

  it("handler 应返回 undefined（空壳行为）", async () => {
    vi.resetModules();
    const { userPromptSubmitHandler } = await import("../handlers.js");

    // 调用 handler，返回值应为 undefined
    const result = await userPromptSubmitHandler({});
    expect(result).toBeUndefined();
  });

  it("handler 接收任意 event 参数时均不报错", async () => {
    vi.resetModules();
    const { userPromptSubmitHandler } = await import("../handlers.js");

    // 测试各种输入不会导致错误
    await expect(userPromptSubmitHandler(null)).resolves.not.toThrow();
    await expect(userPromptSubmitHandler(undefined)).resolves.not.toThrow();
    await expect(
      userPromptSubmitHandler({ prompt: "你好，GhostCode" }),
    ).resolves.not.toThrow();
  });
});

// ============================================
// 测试套件：initializeHooks 集成验证
// ============================================

describe("initializeHooks - UserPromptSubmit 集成", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    clearHooks();
  });

  it("initializeHooks 应同时注册 PreToolUse、Stop 和 UserPromptSubmit", async () => {
    vi.resetModules();
    const [
      { initializeHooks },
      { getHooks: getHooksFresh, clearHooks: clearHooksFresh },
    ] = await Promise.all([
      import("../handlers.js"),
      import("../registry.js"),
    ]);

    clearHooksFresh();
    initializeHooks();

    // 验证三个事件类型均有处理器
    expect(getHooksFresh("PreToolUse").length).toBeGreaterThan(0);
    expect(getHooksFresh("Stop").length).toBeGreaterThan(0);
    expect(getHooksFresh("UserPromptSubmit").length).toBeGreaterThan(0);
  });
});
