/**
 * @file handlers.test.ts
 * @description Hook 处理器单元测试（TDD Red 阶段）
 *              测试 preToolUseHandler、stopHandler、initializeHooks 三个核心函数。
 *              handlers.ts 尚未实现，此文件预期在运行时因模块不存在而失败。
 * @author Atlas.oi
 * @date 2026-03-02
 */

import { describe, it, expect, vi, beforeEach } from "vitest";

// ============================================
// Mock daemon.ts 模块，隔离外部依赖
// 避免测试过程中真正启动或停止 Daemon 进程
// ============================================

vi.mock("../daemon.js", () => ({
  ensureDaemon: vi.fn(),
  stopDaemon: vi.fn(),
  startHeartbeat: vi.fn(),
}));

// Mock session-lease.js，隔离文件系统依赖
// getRefcount 默认返回 0，模拟无其他会话持有 lease 的场景
vi.mock("../session-lease.js", () => ({
  SessionLeaseManager: vi.fn().mockImplementation(() => ({
    acquireLease: vi.fn().mockReturnValue({ leaseId: "test-lease-id", refcount: 1 }),
    releaseLease: vi.fn().mockReturnValue({ refcount: 0, isLast: true }),
    getRefcount: vi.fn().mockReturnValue(0),
  })),
}));

import { ensureDaemon, stopDaemon, startHeartbeat } from "../daemon.js";
import { clearHooks, getHooks } from "./registry.js";

// ============================================
// 测试辅助数据
// ============================================

/** 模拟 AddrDescriptor，与 crates/ghostcode-types/src/addr.rs 格式对应 */
const mockAddr = {
  v: 1,
  transport: "unix",
  path: "/tmp/test.sock",
  pid: 12345,
  version: "0.1.0",
  ts: "2026-03-02T00:00:00.000000Z",
};

// ============================================
// 测试套件
// ============================================

describe("preToolUseHandler", () => {
  beforeEach(async () => {
    // 清除所有 mock 调用记录，避免测试间互相污染
    vi.clearAllMocks();
    // 清除已注册的 Hook，保持测试隔离
    clearHooks();
    // 重置模块状态（清除 handlers.ts 中的模块级缓存变量 daemonPromise 等）
    vi.resetModules();
  });

  it("应在首次调用时调用 ensureDaemon", async () => {
    // 模拟 ensureDaemon 首次成功返回 AddrDescriptor
    (ensureDaemon as ReturnType<typeof vi.fn>).mockResolvedValueOnce(mockAddr);
    // startHeartbeat 返回一个停止函数
    (startHeartbeat as ReturnType<typeof vi.fn>).mockReturnValue(vi.fn());

    // 动态导入确保获取重置后的模块实例
    const { preToolUseHandler } = await import("./handlers.js");

    await preToolUseHandler({});

    // 验证 ensureDaemon 被调用了一次
    expect(ensureDaemon).toHaveBeenCalledTimes(1);
  });

  it("应缓存 ensureDaemon 结果，不重复调用", async () => {
    // 模拟 ensureDaemon 成功返回
    (ensureDaemon as ReturnType<typeof vi.fn>).mockResolvedValue(mockAddr);
    (startHeartbeat as ReturnType<typeof vi.fn>).mockReturnValue(vi.fn());

    const { preToolUseHandler } = await import("./handlers.js");

    // 连续调用两次
    await preToolUseHandler({});
    await preToolUseHandler({});

    // 由于结果已缓存，ensureDaemon 只应被调用一次
    expect(ensureDaemon).toHaveBeenCalledTimes(1);
  });

  it("应在首次成功后启动心跳", async () => {
    // 模拟 ensureDaemon 成功返回
    (ensureDaemon as ReturnType<typeof vi.fn>).mockResolvedValueOnce(mockAddr);
    // 模拟 startHeartbeat 返回停止函数
    const mockStopFn = vi.fn();
    (startHeartbeat as ReturnType<typeof vi.fn>).mockReturnValue(mockStopFn);

    const { preToolUseHandler } = await import("./handlers.js");

    await preToolUseHandler({});

    // 验证 startHeartbeat 被调用，并传入了 AddrDescriptor
    expect(startHeartbeat).toHaveBeenCalledTimes(1);
    expect(startHeartbeat).toHaveBeenCalledWith(mockAddr);
  });

  it("ensureDaemon 失败时不应阻断（静默处理错误）", async () => {
    // 模拟 ensureDaemon 抛出错误
    (ensureDaemon as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
      new Error("Daemon 启动失败")
    );

    const { preToolUseHandler } = await import("./handlers.js");

    // preToolUseHandler 应静默处理错误，不向外抛出
    await expect(preToolUseHandler({})).resolves.not.toThrow();
  });
});

describe("stopHandler", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    clearHooks();
    vi.resetModules();
  });

  it("应调用 stopDaemon", async () => {
    // 模拟 stopDaemon 成功返回
    (stopDaemon as ReturnType<typeof vi.fn>).mockResolvedValueOnce(undefined);

    const { stopHandler } = await import("./handlers.js");

    await stopHandler({});

    // 验证 stopDaemon 被调用
    expect(stopDaemon).toHaveBeenCalledTimes(1);
  });

  it("应停止心跳（如果正在运行）", async () => {
    // 先通过 preToolUseHandler 启动心跳
    const mockStopFn = vi.fn();
    (ensureDaemon as ReturnType<typeof vi.fn>).mockResolvedValueOnce(mockAddr);
    (startHeartbeat as ReturnType<typeof vi.fn>).mockReturnValue(mockStopFn);
    (stopDaemon as ReturnType<typeof vi.fn>).mockResolvedValueOnce(undefined);

    const { preToolUseHandler, stopHandler } = await import("./handlers.js");

    // 先启动 Daemon（触发心跳启动）
    await preToolUseHandler({});
    // 再调用停止处理器
    await stopHandler({});

    // 验证心跳停止函数被调用
    expect(mockStopFn).toHaveBeenCalledTimes(1);
  });

  it("应重置缓存状态，使下次调用重新触发 ensureDaemon", async () => {
    // 第一轮：正常启动
    (ensureDaemon as ReturnType<typeof vi.fn>).mockResolvedValue(mockAddr);
    (startHeartbeat as ReturnType<typeof vi.fn>).mockReturnValue(vi.fn());
    (stopDaemon as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

    const { preToolUseHandler, stopHandler } = await import("./handlers.js");

    await preToolUseHandler({});
    // 停止后缓存应被清除
    await stopHandler({});

    // 重置 mock 调用计数
    vi.clearAllMocks();
    (ensureDaemon as ReturnType<typeof vi.fn>).mockResolvedValue(mockAddr);
    (startHeartbeat as ReturnType<typeof vi.fn>).mockReturnValue(vi.fn());

    // 再次调用 preToolUseHandler，应重新触发 ensureDaemon
    await preToolUseHandler({});

    expect(ensureDaemon).toHaveBeenCalledTimes(1);
  });
});

describe("initializeHooks", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    // 注意：此测试套件不使用 vi.resetModules()，
    // 因为 initializeHooks 依赖与 getHooks 共享同一个 hooks/index.js 实例
    // 使用 clearHooks() 清除注册状态即可
    clearHooks();
  });

  it("应注册 PreToolUse 和 Stop 处理器", async () => {
    // 先重置模块，使 handlers.js 和 index.js 都获得干净的新实例
    vi.resetModules();
    // 动态导入同一个模块上下文中的 handlers 和 index，确保共享同一个 hooks Map
    const [{ initializeHooks }, { getHooks: getHooksFresh, clearHooks: clearHooksFresh }] = await Promise.all([
      import("./handlers.js"),
      import("./registry.js"),
    ]);

    // 确保 hooks 状态干净
    clearHooksFresh();

    // 调用初始化函数注册所有处理器
    initializeHooks();

    // 验证 PreToolUse 类型有处理器注册
    const preToolUseHandlers = getHooksFresh("PreToolUse");
    expect(preToolUseHandlers.length).toBeGreaterThan(0);

    // 验证 Stop 类型有处理器注册
    const stopHandlers = getHooksFresh("Stop");
    expect(stopHandlers.length).toBeGreaterThan(0);
  });
});
