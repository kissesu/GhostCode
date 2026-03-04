/**
 * @file stop-safe.test.ts
 * @description Stop Handler 安全停止（基于 Lease）单元测试
 *              验证多会话共享 Daemon 场景下的安全停止机制：
 *              - preToolUseHandler 成功后应调用 acquireLease
 *              - stopHandler 调用 releaseLease 并传入正确的 leaseId
 *              - 非最后一个 session stop 时不调用 stopDaemon
 *              - 最后一个 session stop 时才调用 stopDaemon
 *              - lease 操作失败时安全降级为执行 shutdown
 * @author Atlas.oi
 * @date 2026-03-04
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
