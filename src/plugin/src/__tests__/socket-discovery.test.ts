/**
 * @file socket-discovery.test.ts
 * @description 测试 resolveSocketPath 三级回退逻辑与 preToolUseHandler 环境变量注入
 *              覆盖以下场景：
 *              1. 显式参数优先返回
 *              2. 无显式参数时从环境变量读取
 *              3. 无环境变量时从 addr.json 文件读取
 *              4. addr.json 不存在时返回 null
 *              5. preToolUseHandler 成功后 process.env.GHOSTCODE_SOCKET_PATH 已设置
 * @author Atlas.oi
 * @date 2026-03-04
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import * as fs from "node:fs";
import * as os from "node:os";

// ============================================
// Mock 文件系统模块，避免真实 I/O
// ============================================
vi.mock("node:fs", () => ({
  readFileSync: vi.fn(),
}));

vi.mock("node:os", () => ({
  homedir: vi.fn(() => "/mock/home"),
}));

// ============================================
// 辅助：保存和恢复环境变量
// ============================================
const SOCKET_ENV_KEY = "GHOSTCODE_SOCKET_PATH";

describe("resolveSocketPath - 三级回退逻辑", () => {
  let originalEnv: string | undefined;

  beforeEach(() => {
    // 保存原始环境变量
    originalEnv = process.env[SOCKET_ENV_KEY];
    // 清空环境变量，确保测试隔离
    delete process.env[SOCKET_ENV_KEY];
    vi.clearAllMocks();
  });

  afterEach(() => {
    // 恢复原始环境变量
    if (originalEnv !== undefined) {
      process.env[SOCKET_ENV_KEY] = originalEnv;
    } else {
      delete process.env[SOCKET_ENV_KEY];
    }
  });

  it("优先级 1：有显式参数时返回显式参数", async () => {
    // 即使环境变量也有值，也应优先使用显式参数
    process.env[SOCKET_ENV_KEY] = "/env/socket.sock";

    const { resolveSocketPath } = await import("../ipc.js");

    const result = resolveSocketPath("/explicit/socket.sock");

    expect(result).toBe("/explicit/socket.sock");
  });

  it("优先级 2：无显式参数时从环境变量读取", async () => {
    process.env[SOCKET_ENV_KEY] = "/env/socket.sock";

    const { resolveSocketPath } = await import("../ipc.js");

    const result = resolveSocketPath();

    expect(result).toBe("/env/socket.sock");
  });

  it("优先级 3：无显式参数和环境变量时从 addr.json 读取", async () => {
    // 环境变量已清空（在 beforeEach 中）
    const mockAddrContent = JSON.stringify({ path: "/addr/socket.sock" });
    vi.mocked(fs.readFileSync).mockReturnValue(mockAddrContent);

    const { resolveSocketPath } = await import("../ipc.js");

    const result = resolveSocketPath();

    expect(result).toBe("/addr/socket.sock");
    // 验证读取了正确的 addr.json 路径
    expect(fs.readFileSync).toHaveBeenCalledWith(
      "/mock/home/.ghostcode/daemon/ghostcoded.addr.json",
      "utf-8"
    );
  });

  it("优先级 4：addr.json 不存在时返回 null", async () => {
    // 环境变量已清空
    // 模拟文件不存在抛出错误
    vi.mocked(fs.readFileSync).mockImplementation(() => {
      throw new Error("ENOENT: no such file or directory");
    });

    const { resolveSocketPath } = await import("../ipc.js");

    const result = resolveSocketPath();

    expect(result).toBeNull();
  });

  it("addr.json 存在但 path 字段缺失时返回 null", async () => {
    // 文件存在但结构不正确（没有 path 字段）
    vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ port: 8080 }));

    const { resolveSocketPath } = await import("../ipc.js");

    const result = resolveSocketPath();

    expect(result).toBeNull();
  });

  it("addr.json 内容不是合法 JSON 时返回 null", async () => {
    vi.mocked(fs.readFileSync).mockReturnValue("not-valid-json{{");

    const { resolveSocketPath } = await import("../ipc.js");

    const result = resolveSocketPath();

    expect(result).toBeNull();
  });
});

// ============================================
// 测试 preToolUseHandler 注入环境变量
// ============================================
describe("preToolUseHandler - 环境变量注入", () => {
  let originalEnv: string | undefined;

  beforeEach(() => {
    originalEnv = process.env[SOCKET_ENV_KEY];
    delete process.env[SOCKET_ENV_KEY];
    vi.clearAllMocks();
    vi.resetModules();
  });

  afterEach(() => {
    if (originalEnv !== undefined) {
      process.env[SOCKET_ENV_KEY] = originalEnv;
    } else {
      delete process.env[SOCKET_ENV_KEY];
    }
    vi.restoreAllMocks();
  });

  it("Daemon 启动成功后应将 addr.path 注入到 process.env.GHOSTCODE_SOCKET_PATH", async () => {
    const mockAddr = { path: "/tmp/ghostcoded.sock", pid: 12345 };

    // Mock daemon 模块
    vi.doMock("../daemon.js", () => ({
      ensureDaemon: vi.fn().mockResolvedValue(mockAddr),
      startHeartbeat: vi.fn().mockReturnValue(() => {}),
      stopDaemon: vi.fn().mockResolvedValue(undefined),
    }));

    // Mock 其他依赖模块（避免真实导入）
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
    vi.doMock("./registry.js", () => ({
      registerHook: vi.fn(),
    }));

    const { preToolUseHandler } = await import("../hooks/handlers.js");

    // 执行前环境变量为空
    expect(process.env[SOCKET_ENV_KEY]).toBeUndefined();

    await preToolUseHandler(null);

    // 执行后环境变量应已设置
    expect(process.env[SOCKET_ENV_KEY]).toBe("/tmp/ghostcoded.sock");
  });

  it("Daemon 启动失败时不应设置 GHOSTCODE_SOCKET_PATH", async () => {
    // Mock daemon 启动失败
    vi.doMock("../daemon.js", () => ({
      ensureDaemon: vi.fn().mockRejectedValue(new Error("Daemon 启动失败")),
      startHeartbeat: vi.fn(),
      stopDaemon: vi.fn(),
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
    vi.doMock("./registry.js", () => ({
      registerHook: vi.fn(),
    }));

    const { preToolUseHandler } = await import("../hooks/handlers.js");

    await preToolUseHandler(null);

    // 启动失败后环境变量不应被设置
    expect(process.env[SOCKET_ENV_KEY]).toBeUndefined();
  });
});
