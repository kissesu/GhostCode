/**
 * @file architecture.test.ts
 * @description Hook 系统架构一致性测试
 *              验证 TypeScript 层的 hooks 与 .mjs 脚本运行时的一致性，以及死代码治理结果：
 *              - handlers.ts 的 userPromptSubmitHandler prompt 提取逻辑与 .mjs 脚本一致
 *              - initializeHooks 不会在模块顶层自动执行（import index.ts 不触发副作用）
 *              - registry.ts 导出的函数均有 @internal JSDoc 标注（通过源码字符串验证）
 * @author Atlas.oi
 * @date 2026-03-05
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

// ============================================
// 测试套件 1：handlers.ts 的 prompt 提取逻辑与 .mjs 脚本一致
// .mjs 脚本使用：event?.event?.prompt || event?.prompt || ""
// handlers.ts 当前使用：直接访问 event.prompt（不支持 event?.event?.prompt 嵌套）
// 验证对齐后 handlers.ts 支持双层嵌套格式
// ============================================

describe("handlers.ts - prompt 提取逻辑与 .mjs 脚本对齐", () => {
  beforeEach(() => {
    vi.resetModules();
  });

  it("应支持顶层 event.prompt 格式（.mjs 的 event?.prompt）", async () => {
    // Mock 依赖模块，使 userPromptSubmitHandler 可以独立测试
    vi.doMock("../../daemon.js", () => ({
      ensureDaemon: vi.fn(),
      stopDaemon: vi.fn(),
      startHeartbeat: vi.fn().mockReturnValue(() => {}),
    }));
    vi.doMock("../../session-lease.js", () => ({
      SessionLeaseManager: vi.fn().mockImplementation(() => ({
        acquireLease: vi.fn(),
        releaseLease: vi.fn(),
      })),
    }));
    vi.doMock("../../learner/index.js", () => ({
      onSessionEnd: vi.fn().mockResolvedValue(undefined),
    }));
    vi.doMock("../../learner/manager.js", () => ({
      appendSessionContent: vi.fn(),
    }));
    vi.doMock("../../keywords/index.js", () => ({
      detectMagicKeywords: vi.fn().mockReturnValue([]),
      resolveKeywordPriority: vi.fn().mockReturnValue(null),
    }));
    vi.doMock("../../keywords/state.js", () => ({
      writeKeywordState: vi.fn().mockResolvedValue(undefined),
    }));

    const { userPromptSubmitHandler } = await import("../handlers.js");

    // 顶层 event.prompt 格式（标准格式）
    const result = await userPromptSubmitHandler({ prompt: "hello world" });

    // 无关键词时返回 undefined（透传不干扰）
    expect(result).toBeUndefined();
  });

  it("应支持嵌套 event.event.prompt 格式（.mjs 的 event?.event?.prompt）", async () => {
    // Mock 依赖模块
    vi.doMock("../../daemon.js", () => ({
      ensureDaemon: vi.fn(),
      stopDaemon: vi.fn(),
      startHeartbeat: vi.fn().mockReturnValue(() => {}),
    }));
    vi.doMock("../../session-lease.js", () => ({
      SessionLeaseManager: vi.fn().mockImplementation(() => ({
        acquireLease: vi.fn(),
        releaseLease: vi.fn(),
      })),
    }));
    vi.doMock("../../learner/index.js", () => ({
      onSessionEnd: vi.fn().mockResolvedValue(undefined),
    }));

    const appendMock = vi.fn();
    vi.doMock("../../learner/manager.js", () => ({
      appendSessionContent: appendMock,
    }));
    vi.doMock("../../keywords/index.js", () => ({
      detectMagicKeywords: vi.fn().mockReturnValue([]),
      resolveKeywordPriority: vi.fn().mockReturnValue(null),
    }));
    vi.doMock("../../keywords/state.js", () => ({
      writeKeywordState: vi.fn().mockResolvedValue(undefined),
    }));

    const { userPromptSubmitHandler } = await import("../handlers.js");

    // 嵌套格式：event.event.prompt（对齐 .mjs 脚本行为）
    const result = await userPromptSubmitHandler({ event: { prompt: "nested prompt" } });

    // 嵌套格式下应能提取到 prompt 并调用 appendSessionContent
    // 如果 handlers.ts 未对齐，此处会返回 undefined 且 appendMock 不被调用
    expect(appendMock).toHaveBeenCalledWith("nested prompt");
    expect(result).toBeUndefined(); // 无关键词时正常返回 undefined
  });

  it("prompt 为空时应返回 undefined 不执行任何操作", async () => {
    vi.doMock("../../daemon.js", () => ({
      ensureDaemon: vi.fn(),
      stopDaemon: vi.fn(),
      startHeartbeat: vi.fn().mockReturnValue(() => {}),
    }));
    vi.doMock("../../session-lease.js", () => ({
      SessionLeaseManager: vi.fn().mockImplementation(() => ({
        acquireLease: vi.fn(),
        releaseLease: vi.fn(),
      })),
    }));
    vi.doMock("../../learner/index.js", () => ({
      onSessionEnd: vi.fn().mockResolvedValue(undefined),
    }));

    const appendMock = vi.fn();
    vi.doMock("../../learner/manager.js", () => ({
      appendSessionContent: appendMock,
    }));
    vi.doMock("../../keywords/index.js", () => ({
      detectMagicKeywords: vi.fn().mockReturnValue([]),
      resolveKeywordPriority: vi.fn().mockReturnValue(null),
    }));
    vi.doMock("../../keywords/state.js", () => ({
      writeKeywordState: vi.fn().mockResolvedValue(undefined),
    }));

    const { userPromptSubmitHandler } = await import("../handlers.js");

    const result = await userPromptSubmitHandler({});
    expect(result).toBeUndefined();
    expect(appendMock).not.toHaveBeenCalled();
  });
});

// ============================================
// 测试套件 2：initializeHooks 不在模块顶层自动执行
// 确保 import hooks/index.ts 不会触发副作用（注册 Hook）
// ============================================

describe("hooks/index.ts - 不自动执行 initializeHooks（无副作用 import）", () => {
  beforeEach(() => {
    vi.resetModules();
  });

  it("import hooks/index.ts 后 getHooks 不应有任何已注册处理器", async () => {
    // Mock 依赖，防止真实 daemon 启动
    vi.doMock("../../daemon.js", () => ({
      ensureDaemon: vi.fn(),
      stopDaemon: vi.fn(),
      startHeartbeat: vi.fn().mockReturnValue(() => {}),
    }));
    vi.doMock("../../session-lease.js", () => ({
      SessionLeaseManager: vi.fn().mockImplementation(() => ({
        acquireLease: vi.fn(),
        releaseLease: vi.fn(),
      })),
    }));
    vi.doMock("../../learner/index.js", () => ({
      onSessionEnd: vi.fn().mockResolvedValue(undefined),
    }));
    vi.doMock("../../learner/manager.js", () => ({
      appendSessionContent: vi.fn(),
    }));
    vi.doMock("../../keywords/index.js", () => ({
      detectMagicKeywords: vi.fn().mockReturnValue([]),
      resolveKeywordPriority: vi.fn().mockReturnValue(null),
    }));
    vi.doMock("../../keywords/state.js", () => ({
      writeKeywordState: vi.fn().mockResolvedValue(undefined),
    }));

    // 直接导入 hooks/index.ts（仅导出，不调用任何函数）
    const { getHooks, clearHooks } = await import("../index.js");

    // 清空注册表，确保测试隔离
    clearHooks();

    // 验证：import 后没有任何 Hook 被自动注册
    // initializeHooks 必须由外部主动调用，不能在模块加载时自动执行
    expect(getHooks("PreToolUse")).toHaveLength(0);
    expect(getHooks("Stop")).toHaveLength(0);
    expect(getHooks("UserPromptSubmit")).toHaveLength(0);
  });

  it("手动调用 initializeHooks 后 getHooks 应返回已注册的处理器", async () => {
    vi.doMock("../../daemon.js", () => ({
      ensureDaemon: vi.fn(),
      stopDaemon: vi.fn(),
      startHeartbeat: vi.fn().mockReturnValue(() => {}),
    }));
    vi.doMock("../../session-lease.js", () => ({
      SessionLeaseManager: vi.fn().mockImplementation(() => ({
        acquireLease: vi.fn(),
        releaseLease: vi.fn(),
      })),
    }));
    vi.doMock("../../learner/index.js", () => ({
      onSessionEnd: vi.fn().mockResolvedValue(undefined),
    }));
    vi.doMock("../../learner/manager.js", () => ({
      appendSessionContent: vi.fn(),
    }));
    vi.doMock("../../keywords/index.js", () => ({
      detectMagicKeywords: vi.fn().mockReturnValue([]),
      resolveKeywordPriority: vi.fn().mockReturnValue(null),
    }));
    vi.doMock("../../keywords/state.js", () => ({
      writeKeywordState: vi.fn().mockResolvedValue(undefined),
    }));

    const { getHooks, clearHooks, initializeHooks } = await import("../index.js");

    // 先清空注册表
    clearHooks();

    // 手动调用 initializeHooks 注册处理器
    initializeHooks();

    // 验证 Hook 已被注册
    expect(getHooks("PreToolUse")).toHaveLength(1);
    expect(getHooks("Stop")).toHaveLength(1);
    expect(getHooks("UserPromptSubmit")).toHaveLength(1);
  });
});

// ============================================
// 测试套件 3：registry.ts 源码中所有导出函数有 @internal 标注
// 通过读取源码字符串验证 JSDoc 标注存在
// ============================================

describe("registry.ts - @internal JSDoc 标注", () => {
  const REGISTRY_PATH = join(
    import.meta.dirname ?? new URL(".", import.meta.url).pathname,
    "..",
    "registry.ts",
  );

  it("registerHook 函数应有 @internal JSDoc 标注", () => {
    const source = readFileSync(REGISTRY_PATH, "utf-8");
    // 验证 @internal 标注出现在 registerHook 函数定义之前的 JSDoc 块中
    expect(source).toMatch(/@internal[\s\S]*?export function registerHook/);
  });

  it("getHooks 函数应有 @internal JSDoc 标注", () => {
    const source = readFileSync(REGISTRY_PATH, "utf-8");
    expect(source).toMatch(/@internal[\s\S]*?export function getHooks/);
  });

  it("clearHooks 函数应有 @internal JSDoc 标注", () => {
    const source = readFileSync(REGISTRY_PATH, "utf-8");
    expect(source).toMatch(/@internal[\s\S]*?export function clearHooks/);
  });
});
