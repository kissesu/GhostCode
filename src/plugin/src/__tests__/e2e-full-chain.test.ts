/**
 * @file 端到端集成测试 - 全链路验证
 * @description 验证 Phase 5 各模块的集成协同：
 *              resolveSocketPath 三级回退 -> 环境变量注入 -> Session Lease 生命周期 -> 安全停止
 * @author Atlas.oi
 * @date 2026-03-04
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import * as path from "node:path";
import * as os from "node:os";
import * as fs from "node:fs";

// 提前导入 SessionLeaseManager 真实实现，避免被后续 vi.doMock 污染
// 这里使用静态 import，在模块 mock 设置之前已完成绑定
import { SessionLeaseManager } from "../session-lease.js";

// ============================================
// 环境变量 Key 常量
// ============================================
const SOCKET_ENV_KEY = "GHOSTCODE_SOCKET_PATH";

// ============================================
// 文件系统 mock 策略：
// - 使用 importOriginal 保留所有真实方法
// - 只将 readFileSync 设为可控 spy（供 resolveSocketPath 测试控制行为）
// - mkdtempSync、rmSync 等保持真实实现，供 Session Lease 测试创建临时目录
// ============================================
vi.mock("node:fs", async (importOriginal) => {
  const actual = await importOriginal<typeof import("node:fs")>();
  return {
    ...actual,
    // 将 readFileSync 设为 spy，默认透传真实实现
    // 各测试套件可以通过 vi.mocked(fs.readFileSync).mockReturnValue(...) 控制行为
    readFileSync: vi.fn((...args: Parameters<typeof actual.readFileSync>) =>
      (actual.readFileSync as (...a: Parameters<typeof actual.readFileSync>) => ReturnType<typeof actual.readFileSync>)(...args)
    ),
  };
});

// mock os.homedir 为固定路径，供 resolveSocketPath 测试使用
// os.tmpdir 保持真实实现（Session Lease 测试需要它）
vi.mock("node:os", async (importOriginal) => {
  const actual = await importOriginal<typeof import("node:os")>();
  return {
    ...actual,
    homedir: vi.fn(() => "/mock/home"),
  };
});

// ============================================
// 测试套件 1：resolveSocketPath 三级回退集成测试
// 覆盖：显式参数 -> 环境变量 -> addr.json -> null
// ============================================
describe("E2E - resolveSocketPath 三级回退集成", () => {
  let originalEnv: string | undefined;

  beforeEach(() => {
    // 保存并清除环境变量，确保测试隔离
    originalEnv = process.env[SOCKET_ENV_KEY];
    delete process.env[SOCKET_ENV_KEY];
    vi.clearAllMocks();
  });

  afterEach(() => {
    // 恢复环境变量
    if (originalEnv !== undefined) {
      process.env[SOCKET_ENV_KEY] = originalEnv;
    } else {
      delete process.env[SOCKET_ENV_KEY];
    }
  });

  it("设置 GHOSTCODE_SOCKET_PATH 后 resolveSocketPath() 返回环境变量值", async () => {
    // 设置环境变量
    process.env[SOCKET_ENV_KEY] = "/tmp/env-provided.sock";

    const { resolveSocketPath } = await import("../ipc.js");

    // 验证环境变量优先返回（不走 addr.json 回退）
    const result = resolveSocketPath();
    expect(result).toBe("/tmp/env-provided.sock");
  });

  it("清除环境变量后 resolveSocketPath() 回退到 addr.json 读取", async () => {
    // 确保环境变量为空
    delete process.env[SOCKET_ENV_KEY];

    // mock readFileSync 返回 addr.json 内容
    vi.mocked(fs.readFileSync).mockReturnValue(
      JSON.stringify({ path: "/tmp/from-addr-json.sock" }) as unknown as Buffer
    );

    const { resolveSocketPath } = await import("../ipc.js");

    // 应从 addr.json 读取路径
    const result = resolveSocketPath();
    expect(result).toBe("/tmp/from-addr-json.sock");
  });

  it("环境变量和 addr.json 都不可用时返回 null", async () => {
    // 确保环境变量为空
    delete process.env[SOCKET_ENV_KEY];

    // mock readFileSync 抛出 ENOENT 错误（文件不存在）
    vi.mocked(fs.readFileSync).mockImplementation(() => {
      throw new Error("ENOENT: no such file or directory");
    });

    const { resolveSocketPath } = await import("../ipc.js");

    // 三级回退均失败，应返回 null
    const result = resolveSocketPath();
    expect(result).toBeNull();
  });

  it("显式参数优先级最高，即使环境变量存在也使用显式参数", async () => {
    // 同时设置环境变量
    process.env[SOCKET_ENV_KEY] = "/tmp/env.sock";

    const { resolveSocketPath } = await import("../ipc.js");

    // 显式参数应覆盖环境变量
    const result = resolveSocketPath("/explicit/path.sock");
    expect(result).toBe("/explicit/path.sock");
  });
});

// ============================================
// 测试套件 2：preToolUseHandler -> 环境变量注入链路
// 覆盖：Daemon 启动成功后 GHOSTCODE_SOCKET_PATH 被正确注入
// ============================================
describe("E2E - preToolUseHandler 环境变量注入链路", () => {
  let originalEnv: string | undefined;

  beforeEach(() => {
    // 重置模块，确保 handlers.ts 模块级状态被清空
    vi.resetModules();
    originalEnv = process.env[SOCKET_ENV_KEY];
    delete process.env[SOCKET_ENV_KEY];
  });

  afterEach(() => {
    // 恢复环境变量
    if (originalEnv !== undefined) {
      process.env[SOCKET_ENV_KEY] = originalEnv;
    } else {
      delete process.env[SOCKET_ENV_KEY];
    }
    vi.restoreAllMocks();
  });

  it("preToolUseHandler 成功后 GHOSTCODE_SOCKET_PATH 被注入，resolveSocketPath 能读取", async () => {
    const mockAddr = {
      v: 1,
      transport: "unix",
      path: "/tmp/test-integration.sock",
      pid: 12345,
      version: "0.1.0",
      ts: new Date().toISOString(),
    };

    // Mock daemon.js：模拟 Daemon 成功启动
    vi.doMock("../daemon.js", () => ({
      ensureDaemon: vi.fn().mockResolvedValue(mockAddr),
      startHeartbeat: vi.fn().mockReturnValue(() => {}),
      stopDaemon: vi.fn().mockResolvedValue(undefined),
    }));

    // Mock 其他 handlers.ts 依赖模块
    vi.doMock("../keywords/index.js", () => ({
      detectMagicKeywords: vi.fn().mockReturnValue([]),
      resolveKeywordPriority: vi.fn().mockReturnValue(null),
    }));
    vi.doMock("../keywords/state.js", () => ({
      writeKeywordState: vi.fn(),
    }));
    vi.doMock("../learner/manager.js", () => ({
      appendSessionContent: vi.fn(),
    }));
    vi.doMock("../hooks/registry.js", () => ({
      registerHook: vi.fn(),
    }));
    vi.doMock("../session-lease.js", () => ({
      SessionLeaseManager: vi.fn().mockImplementation(() => ({
        acquireLease: vi.fn().mockReturnValue({ leaseId: "e2e-lease-1", refcount: 1 }),
        releaseLease: vi.fn().mockReturnValue({ refcount: 0, isLast: true }),
      })),
    }));
    vi.doMock("../learner/index.js", () => ({
      onSessionEnd: vi.fn().mockResolvedValue(undefined),
    }));

    const { preToolUseHandler } = await import("../hooks/handlers.js");

    // 执行前环境变量未设置
    expect(process.env[SOCKET_ENV_KEY]).toBeUndefined();

    // 执行 preToolUseHandler
    await preToolUseHandler({});

    // 验证环境变量已被注入
    expect(process.env[SOCKET_ENV_KEY]).toBe("/tmp/test-integration.sock");

    // 验证注入的路径与 mockAddr.path 完全一致
    expect(process.env[SOCKET_ENV_KEY]).toBe(mockAddr.path);
  });

  it("Daemon 启动失败时 GHOSTCODE_SOCKET_PATH 不会被设置", async () => {
    // Mock daemon.js：模拟 Daemon 启动失败
    vi.doMock("../daemon.js", () => ({
      ensureDaemon: vi.fn().mockRejectedValue(new Error("Daemon 启动失败：二进制文件不存在")),
      startHeartbeat: vi.fn(),
      stopDaemon: vi.fn().mockResolvedValue(undefined),
    }));

    vi.doMock("../keywords/index.js", () => ({
      detectMagicKeywords: vi.fn().mockReturnValue([]),
      resolveKeywordPriority: vi.fn().mockReturnValue(null),
    }));
    vi.doMock("../keywords/state.js", () => ({
      writeKeywordState: vi.fn(),
    }));
    vi.doMock("../learner/manager.js", () => ({
      appendSessionContent: vi.fn(),
    }));
    vi.doMock("../hooks/registry.js", () => ({
      registerHook: vi.fn(),
    }));
    vi.doMock("../session-lease.js", () => ({
      SessionLeaseManager: vi.fn().mockImplementation(() => ({
        acquireLease: vi.fn().mockReturnValue({ leaseId: "e2e-lease-fail", refcount: 1 }),
        releaseLease: vi.fn().mockReturnValue({ refcount: 0, isLast: true }),
      })),
    }));
    vi.doMock("../learner/index.js", () => ({
      onSessionEnd: vi.fn().mockResolvedValue(undefined),
    }));

    const { preToolUseHandler } = await import("../hooks/handlers.js");

    // 执行 preToolUseHandler（应静默处理错误，不抛出）
    await expect(preToolUseHandler({})).resolves.toBeUndefined();

    // Daemon 启动失败时不应设置环境变量
    expect(process.env[SOCKET_ENV_KEY]).toBeUndefined();
  });
});

// ============================================
// 测试套件 3：Session Lease 生命周期集成测试
// 覆盖：两个 session acquire -> 一次 release（isLast=false）-> 最后 release（isLast=true）
// 注意：本套件使用真实文件 I/O（临时目录），验证 SessionLeaseManager 的实际存储逻辑
// ============================================
describe("E2E - Session Lease 生命周期", () => {
  let tmpDir: string;
  let sessionsFilePath: string;

  beforeEach(() => {
    // 注意：此套件使用顶层静态 import 的真实 SessionLeaseManager，不调用 vi.resetModules()
    // 避免重置模块后静态 import 绑定丢失
    // 创建临时目录存放 sessions.json
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "ghostcode-e2e-test-"));
    sessionsFilePath = path.join(tmpDir, "sessions.json");
  });

  afterEach(() => {
    // 清理临时目录
    try {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    } catch {
      // 清理失败不阻断测试
    }
    vi.clearAllMocks();
  });

  it("两个 session acquire -> 第一个 release（isLast=false）-> 最后 release（isLast=true）", async () => {
    // 使用顶层静态 import 的真实 SessionLeaseManager（不受 doMock 污染）
    const manager = new SessionLeaseManager(sessionsFilePath);

    // ============================================
    // 第一步：Session A acquire
    // ============================================
    const leaseA = manager.acquireLease();
    expect(leaseA.leaseId).toBeTruthy();
    expect(leaseA.refcount).toBe(1);

    // ============================================
    // 第二步：Session B acquire
    // ============================================
    const leaseB = manager.acquireLease();
    expect(leaseB.leaseId).toBeTruthy();
    // 两个 leaseId 应不同（UUID v4 保证唯一性）
    expect(leaseB.leaseId).not.toBe(leaseA.leaseId);
    expect(leaseB.refcount).toBe(2);

    // 验证当前引用计数为 2
    expect(manager.getRefcount()).toBe(2);

    // ============================================
    // 第三步：Session A release（非最后一个）
    // ============================================
    const releaseA = manager.releaseLease(leaseA.leaseId);
    expect(releaseA.refcount).toBe(1);
    expect(releaseA.isLast).toBe(false);

    // 验证当前引用计数降为 1
    expect(manager.getRefcount()).toBe(1);

    // ============================================
    // 第四步：Session B release（最后一个）
    // ============================================
    const releaseB = manager.releaseLease(leaseB.leaseId);
    expect(releaseB.refcount).toBe(0);
    expect(releaseB.isLast).toBe(true);

    // 验证最终引用计数为 0
    expect(manager.getRefcount()).toBe(0);
  });

  it("引用计数始终与实际 session 数量一致", async () => {
    const manager = new SessionLeaseManager(sessionsFilePath);

    // 模拟 3 个 session 依次 acquire
    const leases: string[] = [];
    for (let i = 0; i < 3; i++) {
      const info = manager.acquireLease();
      leases.push(info.leaseId);
      // 每次 acquire 后 refcount 应该等于已 acquire 数量
      expect(info.refcount).toBe(i + 1);
      expect(manager.getRefcount()).toBe(i + 1);
    }

    // 依次释放，验证 refcount 递减
    for (let i = 0; i < leases.length; i++) {
      const result = manager.releaseLease(leases[i]);
      const remaining = leases.length - i - 1;
      expect(result.refcount).toBe(remaining);
      expect(result.isLast).toBe(remaining === 0);
    }
  });

  it("释放不存在的 leaseId 时不报错，refcount 不变", async () => {
    const manager = new SessionLeaseManager(sessionsFilePath);

    // acquire 一个 lease
    const lease = manager.acquireLease();
    expect(manager.getRefcount()).toBe(1);

    // 尝试释放一个不存在的 leaseId（不应抛出异常）
    const result = manager.releaseLease("non-existent-lease-id");
    // wasRemoved=false 时 isLast 为 false
    expect(result.isLast).toBe(false);
    // refcount 不受影响
    expect(manager.getRefcount()).toBe(1);

    // 正常释放真实 lease
    const finalResult = manager.releaseLease(lease.leaseId);
    expect(finalResult.isLast).toBe(true);
    expect(manager.getRefcount()).toBe(0);
  });
});

// ============================================
// 测试套件 4：Stop Handler 安全停止集成测试
// 覆盖：preToolUseHandler acquire -> stopHandler release -> 单 session 时 stopDaemon 被调用
// ============================================
describe("E2E - Stop Handler 安全停止集成", () => {
  beforeEach(() => {
    // 每次测试重置模块，清空 handlers.ts 的模块级状态
    vi.resetModules();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("单 session 场景：preToolUseHandler 后 stopHandler 应调用 stopDaemon（isLast=true）", async () => {
    const leaseId = "e2e-stop-test-lease";
    const mockStopDaemon = vi.fn().mockResolvedValue(undefined);
    const mockAcquire = vi.fn().mockReturnValue({ leaseId, refcount: 1 });
    const mockRelease = vi.fn().mockReturnValue({ refcount: 0, isLast: true });

    // Mock daemon.js
    vi.doMock("../daemon.js", () => ({
      ensureDaemon: vi.fn().mockResolvedValue({
        v: 1,
        transport: "unix",
        path: "/tmp/e2e-stop.sock",
        pid: 99999,
        version: "0.1.0",
        ts: new Date().toISOString(),
      }),
      startHeartbeat: vi.fn().mockReturnValue(() => {}),
      stopDaemon: mockStopDaemon,
    }));

    // Mock session-lease.js
    vi.doMock("../session-lease.js", () => ({
      SessionLeaseManager: vi.fn().mockImplementation(() => ({
        acquireLease: mockAcquire,
        releaseLease: mockRelease,
      })),
    }));

    // Mock 其他依赖
    vi.doMock("../keywords/index.js", () => ({
      detectMagicKeywords: vi.fn().mockReturnValue([]),
      resolveKeywordPriority: vi.fn().mockReturnValue(null),
    }));
    vi.doMock("../keywords/state.js", () => ({
      writeKeywordState: vi.fn(),
    }));
    vi.doMock("../learner/manager.js", () => ({
      appendSessionContent: vi.fn(),
    }));
    vi.doMock("../hooks/registry.js", () => ({
      registerHook: vi.fn(),
    }));
    vi.doMock("../learner/index.js", () => ({
      onSessionEnd: vi.fn().mockResolvedValue(undefined),
    }));

    const { preToolUseHandler, stopHandler } = await import("../hooks/handlers.js");

    // 第一步：preToolUseHandler 获取 lease
    await preToolUseHandler({});

    // 验证 acquireLease 被调用
    expect(mockAcquire).toHaveBeenCalledTimes(1);

    // 第二步：stopHandler 释放 lease 并停止 Daemon
    await stopHandler({});

    // 验证 releaseLease 被调用（传入正确 leaseId）
    expect(mockRelease).toHaveBeenCalledTimes(1);
    expect(mockRelease).toHaveBeenCalledWith(leaseId);

    // 单 session 时 isLast=true，应调用 stopDaemon
    expect(mockStopDaemon).toHaveBeenCalledTimes(1);
  });

  it("多 session 场景：非最后 session stop 时不调用 stopDaemon（isLast=false）", async () => {
    const mockStopDaemon = vi.fn().mockResolvedValue(undefined);
    // 模拟还有其他 session 在运行，release 返回 isLast=false
    const mockRelease = vi.fn().mockReturnValue({ refcount: 1, isLast: false });

    vi.doMock("../daemon.js", () => ({
      ensureDaemon: vi.fn().mockResolvedValue({
        v: 1,
        transport: "unix",
        path: "/tmp/e2e-multi.sock",
        pid: 88888,
        version: "0.1.0",
        ts: new Date().toISOString(),
      }),
      startHeartbeat: vi.fn().mockReturnValue(() => {}),
      stopDaemon: mockStopDaemon,
    }));

    vi.doMock("../session-lease.js", () => ({
      SessionLeaseManager: vi.fn().mockImplementation(() => ({
        acquireLease: vi.fn().mockReturnValue({ leaseId: "multi-session-lease", refcount: 2 }),
        releaseLease: mockRelease,
      })),
    }));

    vi.doMock("../keywords/index.js", () => ({
      detectMagicKeywords: vi.fn().mockReturnValue([]),
      resolveKeywordPriority: vi.fn().mockReturnValue(null),
    }));
    vi.doMock("../keywords/state.js", () => ({
      writeKeywordState: vi.fn(),
    }));
    vi.doMock("../learner/manager.js", () => ({
      appendSessionContent: vi.fn(),
    }));
    vi.doMock("../hooks/registry.js", () => ({
      registerHook: vi.fn(),
    }));
    vi.doMock("../learner/index.js", () => ({
      onSessionEnd: vi.fn().mockResolvedValue(undefined),
    }));

    const { preToolUseHandler, stopHandler } = await import("../hooks/handlers.js");

    await preToolUseHandler({});
    await stopHandler({});

    // isLast=false，不应调用 stopDaemon
    expect(mockStopDaemon).not.toHaveBeenCalled();
  });
});

// ============================================
// 测试套件 5：callDaemon 使用 resolveSocketPath 连接失败场景
// 覆盖：设置不存在的 socket 路径后 callDaemon 抛出 IpcConnectionError
// ============================================
describe("E2E - callDaemon 连接失败链路", () => {
  let originalEnv: string | undefined;

  beforeEach(() => {
    originalEnv = process.env[SOCKET_ENV_KEY];
    // 设置为不存在的 socket 路径，确保连接会失败
    process.env[SOCKET_ENV_KEY] = "/tmp/ghostcode-nonexistent-e2e-test-12345.sock";
    vi.clearAllMocks();
  });

  afterEach(() => {
    if (originalEnv !== undefined) {
      process.env[SOCKET_ENV_KEY] = originalEnv;
    } else {
      delete process.env[SOCKET_ENV_KEY];
    }
  });

  it("callDaemon 连接不存在的 socket 应抛出 IpcConnectionError，错误消息包含路径", async () => {
    const { callDaemon, IpcConnectionError, resetClient } = await import("../ipc.js");

    // 重置客户端，确保使用最新的环境变量
    await resetClient();

    // callDaemon 应在连接失败后抛出 IpcConnectionError
    // 注意：这是真实的连接尝试（socket 不存在），不需要 Daemon 运行
    await expect(callDaemon("ping")).rejects.toSatisfy((err: unknown) => {
      return err instanceof IpcConnectionError;
    });
  }, 15_000); // 等待重连超时（最多 100+200+400ms 三次重连）

  it("callDaemon 错误消息应包含不存在的 socket 路径信息", async () => {
    const { callDaemon, IpcConnectionError, resetClient } = await import("../ipc.js");

    await resetClient();

    let caughtError: unknown;
    try {
      await callDaemon("ping");
    } catch (err) {
      caughtError = err;
    }

    // 验证错误是 IpcConnectionError 实例
    expect(caughtError).toBeInstanceOf(IpcConnectionError);

    // 验证错误消息包含 socket 路径信息
    const errorMessage = (caughtError as Error).message;
    expect(errorMessage).toContain("ghostcode-nonexistent-e2e-test-12345");
  }, 15_000);
});
