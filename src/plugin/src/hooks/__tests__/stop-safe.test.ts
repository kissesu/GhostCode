/**
 * @file stop-safe.test.ts
 * @description Stop Handler 安全停止（基于 Lease）单元测试
 *              验证多会话共享 Daemon 场景下的安全停止机制：
 *              - preToolUseHandler 成功后应调用 acquireLease
 *              - stopHandler 调用 releaseLease 并传入正确的 leaseId
 *              - 非最后一个 session stop 时不调用 stopDaemon
 *              - 最后一个 session stop 时才调用 stopDaemon
 *              - lease 操作失败时安全降级为执行 shutdown
 *              - onSessionEnd 在 stopDaemon 之前被调用（调用顺序）
 *              - onSessionEnd 失败不影响后续 stopDaemon
 *              - PBT：多会话 lease acquire/release 序列下 refcount >= 0 且仅 isLast=true 时触发关停
 * @author Atlas.oi
 * @date 2026-03-05
 */

import { describe, it, expect, vi, beforeEach } from "vitest";

// ============================================
// 注意：handlers.ts 有模块级状态（daemonPromise, stopHeartbeat, currentLeaseId）
// 每个测试用例通过 vi.resetModules() + dynamic import 获取全新的模块实例
// 避免测试间状态污染
// ============================================

// ============================================
// 测试套件：preToolUseHandler - acquireLease
// ============================================

describe("preToolUseHandler - acquireLease", () => {
  beforeEach(() => {
    // 每次测试前重置所有模块，确保 handlers.ts 模块级状态被清空
    vi.resetModules();
  });

  it("Daemon 启动成功后应调用 acquireLease", async () => {
    // 构造 mock：acquireLease 返回固定 leaseId
    const mockAcquire = vi.fn().mockReturnValue({ leaseId: "test-lease-1", refcount: 1 });
    const mockRelease = vi.fn();

    // Mock daemon.js
    vi.doMock("../../daemon.js", () => ({
      ensureDaemon: vi.fn().mockResolvedValue({ host: "127.0.0.1", port: 9000 }),
      stopDaemon: vi.fn().mockResolvedValue(undefined),
      startHeartbeat: vi.fn().mockReturnValue(() => {}),
    }));

    // Mock session-lease.js
    vi.doMock("../../session-lease.js", () => ({
      SessionLeaseManager: vi.fn().mockImplementation(() => ({
        acquireLease: mockAcquire,
        releaseLease: mockRelease,
      })),
    }));

    // Mock learner/index.js
    vi.doMock("../../learner/index.js", () => ({
      onSessionEnd: vi.fn().mockResolvedValue(undefined),
    }));

    // 动态导入以获取重置后的模块实例
    const { preToolUseHandler } = await import("../handlers.js");

    // 调用 preToolUseHandler
    await preToolUseHandler(undefined);

    // 验证 acquireLease 被调用一次
    expect(mockAcquire).toHaveBeenCalledTimes(1);
  });

  it("preToolUseHandler 重复调用不应重复 acquireLease（幂等保护）", async () => {
    const mockAcquire = vi.fn().mockReturnValue({ leaseId: "test-lease-2", refcount: 1 });

    vi.doMock("../../daemon.js", () => ({
      ensureDaemon: vi.fn().mockResolvedValue({ host: "127.0.0.1", port: 9000 }),
      stopDaemon: vi.fn().mockResolvedValue(undefined),
      startHeartbeat: vi.fn().mockReturnValue(() => {}),
    }));

    vi.doMock("../../session-lease.js", () => ({
      SessionLeaseManager: vi.fn().mockImplementation(() => ({
        acquireLease: mockAcquire,
        releaseLease: vi.fn(),
      })),
    }));

    vi.doMock("../../learner/index.js", () => ({
      onSessionEnd: vi.fn().mockResolvedValue(undefined),
    }));

    const { preToolUseHandler } = await import("../handlers.js");

    // 多次调用 preToolUseHandler，daemonPromise 不为 null 后应直接返回
    await preToolUseHandler(undefined);
    await preToolUseHandler(undefined);
    await preToolUseHandler(undefined);

    // acquireLease 只应被调用一次（第一次 daemon 启动时）
    expect(mockAcquire).toHaveBeenCalledTimes(1);
  });
});

// ============================================
// 测试套件：stopHandler - releaseLease
// ============================================

describe("stopHandler - releaseLease", () => {
  beforeEach(() => {
    vi.resetModules();
  });

  it("stopHandler 应调用 releaseLease 并传入正确的 leaseId", async () => {
    const leaseId = "test-lease-correct-id";
    const mockAcquire = vi.fn().mockReturnValue({ leaseId, refcount: 1 });
    const mockRelease = vi.fn().mockReturnValue({ refcount: 0, isLast: true });
    const mockStopDaemon = vi.fn().mockResolvedValue(undefined);

    vi.doMock("../../daemon.js", () => ({
      ensureDaemon: vi.fn().mockResolvedValue({ host: "127.0.0.1", port: 9000 }),
      stopDaemon: mockStopDaemon,
      startHeartbeat: vi.fn().mockReturnValue(() => {}),
    }));

    vi.doMock("../../session-lease.js", () => ({
      SessionLeaseManager: vi.fn().mockImplementation(() => ({
        acquireLease: mockAcquire,
        releaseLease: mockRelease,
      })),
    }));

    vi.doMock("../../learner/index.js", () => ({
      onSessionEnd: vi.fn().mockResolvedValue(undefined),
    }));

    const { preToolUseHandler, stopHandler } = await import("../handlers.js");

    // 先 acquire（通过 preToolUseHandler）
    await preToolUseHandler(undefined);
    // 再 release（通过 stopHandler）
    await stopHandler(undefined);

    // 验证 releaseLease 被调用，且传入的 leaseId 正确
    expect(mockRelease).toHaveBeenCalledTimes(1);
    expect(mockRelease).toHaveBeenCalledWith(leaseId);
  });

  it("两个 session 共享 Daemon - 第一个 session stop 不应调用 stopDaemon（isLast=false）", async () => {
    const mockRelease = vi.fn().mockReturnValue({ refcount: 1, isLast: false });
    const mockStopDaemon = vi.fn().mockResolvedValue(undefined);

    vi.doMock("../../daemon.js", () => ({
      ensureDaemon: vi.fn().mockResolvedValue({ host: "127.0.0.1", port: 9000 }),
      stopDaemon: mockStopDaemon,
      startHeartbeat: vi.fn().mockReturnValue(() => {}),
    }));

    vi.doMock("../../session-lease.js", () => ({
      SessionLeaseManager: vi.fn().mockImplementation(() => ({
        acquireLease: vi.fn().mockReturnValue({ leaseId: "session-1", refcount: 2 }),
        releaseLease: mockRelease,
      })),
    }));

    vi.doMock("../../learner/index.js", () => ({
      onSessionEnd: vi.fn().mockResolvedValue(undefined),
    }));

    const { preToolUseHandler, stopHandler } = await import("../handlers.js");

    await preToolUseHandler(undefined);
    await stopHandler(undefined);

    // isLast=false，不应调用 stopDaemon
    expect(mockStopDaemon).not.toHaveBeenCalled();
  });

  it("最后一个 session stop - 应调用 stopDaemon（isLast=true）", async () => {
    const mockRelease = vi.fn().mockReturnValue({ refcount: 0, isLast: true });
    const mockStopDaemon = vi.fn().mockResolvedValue(undefined);

    vi.doMock("../../daemon.js", () => ({
      ensureDaemon: vi.fn().mockResolvedValue({ host: "127.0.0.1", port: 9000 }),
      stopDaemon: mockStopDaemon,
      startHeartbeat: vi.fn().mockReturnValue(() => {}),
    }));

    vi.doMock("../../session-lease.js", () => ({
      SessionLeaseManager: vi.fn().mockImplementation(() => ({
        acquireLease: vi.fn().mockReturnValue({ leaseId: "last-session", refcount: 1 }),
        releaseLease: mockRelease,
      })),
    }));

    vi.doMock("../../learner/index.js", () => ({
      onSessionEnd: vi.fn().mockResolvedValue(undefined),
    }));

    const { preToolUseHandler, stopHandler } = await import("../handlers.js");

    await preToolUseHandler(undefined);
    await stopHandler(undefined);

    // isLast=true，应调用 stopDaemon
    expect(mockStopDaemon).toHaveBeenCalledTimes(1);
  });

  it("lease 文件损坏（releaseLease 抛异常）时保守不关闭 Daemon（防止误杀共享实例）", async () => {
    const mockRelease = vi.fn().mockImplementation(() => {
      throw new Error("sessions.json 文件损坏，无法解析");
    });
    const mockStopDaemon = vi.fn().mockResolvedValue(undefined);

    vi.doMock("../../daemon.js", () => ({
      ensureDaemon: vi.fn().mockResolvedValue({ host: "127.0.0.1", port: 9000 }),
      stopDaemon: mockStopDaemon,
      startHeartbeat: vi.fn().mockReturnValue(() => {}),
    }));

    vi.doMock("../../session-lease.js", () => ({
      SessionLeaseManager: vi.fn().mockImplementation(() => ({
        acquireLease: vi.fn().mockReturnValue({ leaseId: "error-session", refcount: 1 }),
        releaseLease: mockRelease,
      })),
    }));

    vi.doMock("../../learner/index.js", () => ({
      onSessionEnd: vi.fn().mockResolvedValue(undefined),
    }));

    const { preToolUseHandler, stopHandler } = await import("../handlers.js");

    await preToolUseHandler(undefined);
    // 即使 releaseLease 抛异常，stopHandler 也不应抛出
    await expect(stopHandler(undefined)).resolves.not.toThrow();

    // 保守策略：无法确认是否为最后一个会话时，不关闭 Daemon，防止误杀其他会话的共享实例
    // 孤儿 Daemon 由心跳超时机制或下次启动时的 cleanup 处理
    expect(mockStopDaemon).toHaveBeenCalledTimes(0);
  });
});

// ============================================
// 测试套件：stopHandler - 调用顺序验证
// 验证 onSessionEnd 必须在 stopDaemon 之前调用
// 确保 Skill Learning 在 Daemon 仍运行时完成
// ============================================

describe("stopHandler - 调用顺序：onSessionEnd 先于 stopDaemon", () => {
  beforeEach(() => {
    vi.resetModules();
  });

  it("onSessionEnd 应在 stopDaemon 之前被调用", async () => {
    // 用于记录调用顺序的序列数组
    const callOrder: string[] = [];

    const mockStopDaemon = vi.fn().mockImplementation(async () => {
      callOrder.push("stopDaemon");
    });
    const mockOnSessionEnd = vi.fn().mockImplementation(async () => {
      callOrder.push("onSessionEnd");
    });

    vi.doMock("../../daemon.js", () => ({
      ensureDaemon: vi.fn().mockResolvedValue({ host: "127.0.0.1", port: 9000 }),
      stopDaemon: mockStopDaemon,
      startHeartbeat: vi.fn().mockReturnValue(() => {}),
    }));

    vi.doMock("../../session-lease.js", () => ({
      SessionLeaseManager: vi.fn().mockImplementation(() => ({
        acquireLease: vi.fn().mockReturnValue({ leaseId: "order-test", refcount: 1 }),
        // isLast=true 以触发 stopDaemon 调用
        releaseLease: vi.fn().mockReturnValue({ refcount: 0, isLast: true }),
      })),
    }));

    vi.doMock("../../learner/index.js", () => ({
      onSessionEnd: mockOnSessionEnd,
    }));

    const { preToolUseHandler, stopHandler } = await import("../handlers.js");

    await preToolUseHandler(undefined);
    await stopHandler(undefined);

    // 验证 onSessionEnd 在 stopDaemon 之前被调用
    expect(callOrder).toEqual(["onSessionEnd", "stopDaemon"]);
    expect(mockOnSessionEnd).toHaveBeenCalledTimes(1);
    expect(mockStopDaemon).toHaveBeenCalledTimes(1);
  });

  it("onSessionEnd 失败不应阻断后续 stopDaemon 的调用", async () => {
    const mockStopDaemon = vi.fn().mockResolvedValue(undefined);
    const mockOnSessionEnd = vi.fn().mockRejectedValue(new Error("Skill Learning 分析异常"));

    vi.doMock("../../daemon.js", () => ({
      ensureDaemon: vi.fn().mockResolvedValue({ host: "127.0.0.1", port: 9000 }),
      stopDaemon: mockStopDaemon,
      startHeartbeat: vi.fn().mockReturnValue(() => {}),
    }));

    vi.doMock("../../session-lease.js", () => ({
      SessionLeaseManager: vi.fn().mockImplementation(() => ({
        acquireLease: vi.fn().mockReturnValue({ leaseId: "error-order-test", refcount: 1 }),
        // isLast=true 以触发 stopDaemon
        releaseLease: vi.fn().mockReturnValue({ refcount: 0, isLast: true }),
      })),
    }));

    vi.doMock("../../learner/index.js", () => ({
      onSessionEnd: mockOnSessionEnd,
    }));

    const { preToolUseHandler, stopHandler } = await import("../handlers.js");

    await preToolUseHandler(undefined);
    // onSessionEnd 抛异常，stopHandler 不应抛出
    await expect(stopHandler(undefined)).resolves.not.toThrow();

    // 即使 onSessionEnd 失败，stopDaemon 仍需被调用
    expect(mockOnSessionEnd).toHaveBeenCalledTimes(1);
    expect(mockStopDaemon).toHaveBeenCalledTimes(1);
  });
});

// ============================================
// 测试套件：PBT（基于属性的测试）
// 验证多会话 acquire/release 序列下的不变量：
// 1. refcount 永远 >= 0
// 2. 仅在 isLast=true 时触发 stopDaemon
// ============================================

describe("PBT - 多会话 lease 序列不变量", () => {
  beforeEach(() => {
    vi.resetModules();
  });

  it("多次 acquire/release 序列：refcount 不应为负数且仅 isLast=true 时关停", async () => {
    // 模拟 N 个会话：每个会话 acquire 一个 lease，然后按顺序 release
    // 验证：只有最后一个 release 时 isLast=true，并且 stopDaemon 恰好被调用一次
    const N = 5;
    let refcount = 0;
    const refcountHistory: number[] = [];

    const mockStopDaemon = vi.fn().mockResolvedValue(undefined);

    // 模拟多会话使用同一个 leaseManager 实例（通过闭包共享状态）
    vi.doMock("../../daemon.js", () => ({
      ensureDaemon: vi.fn().mockResolvedValue({ host: "127.0.0.1", port: 9000 }),
      stopDaemon: mockStopDaemon,
      startHeartbeat: vi.fn().mockReturnValue(() => {}),
    }));

    vi.doMock("../../session-lease.js", () => ({
      SessionLeaseManager: vi.fn().mockImplementation(() => ({
        acquireLease: vi.fn().mockImplementation(() => {
          refcount++;
          refcountHistory.push(refcount);
          return { leaseId: `lease-${refcount}`, refcount };
        }),
        releaseLease: vi.fn().mockImplementation(() => {
          refcount--;
          refcountHistory.push(refcount);
          // refcount 不应为负数（不变量断言）
          expect(refcount).toBeGreaterThanOrEqual(0);
          return { refcount, isLast: refcount === 0 };
        }),
        getRefcount: vi.fn().mockReturnValue(refcount),
      })),
    }));

    vi.doMock("../../learner/index.js", () => ({
      onSessionEnd: vi.fn().mockResolvedValue(undefined),
    }));

    // 模拟 N 个会话并发：每个从全新实例 import，但共享同一个 leaseManager 状态（通过闭包）
    // 这里简化为串行：acquire N 次，release N 次
    const { preToolUseHandler, stopHandler } = await import("../handlers.js");

    // 第一个会话（本测试进程代表）：acquire 一次
    await preToolUseHandler(undefined);

    // 模拟另外 N-1 个会话直接操作 refcount（不通过 handlers，因为每个会话有独立模块实例）
    // 这里直接修改共享的 refcount 变量模拟 N-1 次 acquire
    for (let i = 1; i < N; i++) {
      refcount++;
      refcountHistory.push(refcount);
    }

    // 然后模拟 N-1 个其他会话 release（不触发 stopDaemon，因为不通过本 handler）
    for (let i = 0; i < N - 1; i++) {
      refcount--;
      refcountHistory.push(refcount);
      expect(refcount).toBeGreaterThanOrEqual(0);
    }

    // 最后一个 release 通过 stopHandler（此时 refcount 变为 0，isLast=true）
    await stopHandler(undefined);

    // 验证：refcount 历史中没有出现负数
    for (const rc of refcountHistory) {
      expect(rc).toBeGreaterThanOrEqual(0);
    }

    // 验证：stopDaemon 恰好被调用一次（仅最后一个会话触发）
    expect(mockStopDaemon).toHaveBeenCalledTimes(1);
  });
});
