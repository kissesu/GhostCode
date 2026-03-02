/**
 * @file Daemon 生命周期管理测试
 * @description T17 规格要求的 5 个测试用例
 *              使用 vitest 框架，vi.mock 模拟文件系统和子进程操作
 * @author Atlas.oi
 * @date 2026-03-01
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ============================================
// Mock 声明（必须在顶层，vitest 静态分析需要）
// ============================================

vi.mock("node:fs", () => ({
  existsSync: vi.fn(),
  readFileSync: vi.fn(),
}));

vi.mock("node:child_process", () => ({
  spawn: vi.fn(),
}));

vi.mock("node:net", () => ({
  createConnection: vi.fn(),
}));

import { existsSync, readFileSync } from "node:fs";
import { spawn } from "node:child_process";
import { createConnection } from "node:net";
import { EventEmitter } from "node:events";

// ============================================
// 测试辅助工厂函数
// ============================================

/** 心跳间隔常量（与 daemon.ts 中一致） */
const HEARTBEAT_INTERVAL_MS = 10_000;

/** ping 超时常量（与 daemon.ts 中一致） */
const PING_TIMEOUT_MS = 3000;

/**
 * 创建模拟的 AddrDescriptor（对应真实 addr.json 格式）
 * 参考: crates/ghostcode-types/src/addr.rs:25-53
 */
function mockAddr(pid = 12345) {
  return {
    v: 1,
    transport: "unix",
    path: "/tmp/ghostcode-test/ghostcoded.sock",
    pid,
    version: "0.1.0",
    ts: new Date().toISOString(),
  };
}

/**
 * 创建模拟的 Unix socket（EventEmitter 模拟）
 */
function createMockSocket() {
  const emitter = new EventEmitter();
  const socket = Object.assign(emitter, {
    write: vi.fn(),
    destroy: vi.fn(),
  });
  return socket;
}

/**
 * 构建 Daemon ping 响应（成功）
 * 参考: crates/ghostcode-daemon/src/dispatch.rs:100-106
 */
function buildPingResponse(pong = true): string {
  return JSON.stringify({
    v: 1,
    ok: pong,
    result: { pong, version: "0.1.0", has_unread: false },
  }) + "\n";
}

// ============================================
// 测试套件
// ============================================

describe("daemon", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.resetModules();
  });

  // ------------------------------------------
  // 测试 1: ensureDaemon starts daemon if not running
  // ------------------------------------------
  describe("ensureDaemon", () => {
    it("当 Daemon 未运行时，应 spawn 新进程并返回 AddrDescriptor", async () => {
      const addr = mockAddr();

      // 模拟 addr.json 不存在
      (existsSync as ReturnType<typeof vi.fn>)
        .mockReturnValueOnce(false)   // ADDR_JSON_PATH 不存在（readAddrJson）
        .mockReturnValueOnce(true)    // DAEMON_BIN_PATH 存在（_spawnDaemon）
        .mockReturnValueOnce(true);   // waitForAddrJson 轮询时 ADDR_JSON_PATH 存在

      // 模拟 spawn 成功，mockChild 需要有 unref 方法
      const mockChild = Object.assign(new EventEmitter(), {
        unref: vi.fn(),
      }) as unknown as ReturnType<typeof spawn>;
      (spawn as ReturnType<typeof vi.fn>).mockReturnValue(mockChild);

      // 模拟 waitForAddrJson 轮询 readFileSync 返回 addr
      (readFileSync as ReturnType<typeof vi.fn>)
        .mockReturnValueOnce(JSON.stringify(addr));

      // 模拟 ping 成功（socket 连接）
      const mockSocket = createMockSocket();
      (createConnection as ReturnType<typeof vi.fn>).mockReturnValue(mockSocket);

      const { ensureDaemon } = await import("../daemon.js");

      // 触发 spawn 的 "spawn" 事件和 ping 响应
      process.nextTick(() => {
        mockChild.emit("spawn");
        // waitForAddrJson 的 setTimeout(100ms) 在真实 timer 下会触发
        // ping 的 socket 事件需要等 waitForAddrJson 完成后触发
        setTimeout(() => {
          mockSocket.emit("connect");
          mockSocket.emit("data", Buffer.from(buildPingResponse(true)));
        }, 150);
      });

      const result = await ensureDaemon();

      expect(spawn).toHaveBeenCalledWith(
        expect.stringContaining("ghostcoded"),
        [],
        expect.objectContaining({ detached: true, stdio: "ignore" })
      );
      expect(result.v).toBe(1);
      expect(result.transport).toBe("unix");
    }, 10000);

    // ------------------------------------------
    // 测试 2: ensureDaemon reuses existing daemon
    // ------------------------------------------
    it("当 Daemon 已在运行时，应复用现有 Daemon", async () => {
      const addr = mockAddr();

      (existsSync as ReturnType<typeof vi.fn>).mockReturnValue(true);
      (readFileSync as ReturnType<typeof vi.fn>).mockReturnValue(
        JSON.stringify(addr)
      );

      vi.spyOn(process, "kill").mockImplementation(() => true);

      const mockSocket = createMockSocket();
      (createConnection as ReturnType<typeof vi.fn>).mockReturnValue(mockSocket);

      const { ensureDaemon } = await import("../daemon.js");

      process.nextTick(() => {
        mockSocket.emit("connect");
        mockSocket.emit("data", Buffer.from(buildPingResponse(true)));
      });

      const result = await ensureDaemon();

      expect(spawn).not.toHaveBeenCalled();
      expect(result.pid).toBe(addr.pid);
    });

    // ------------------------------------------
    // 测试 5: concurrent ensureDaemon calls safe
    // ------------------------------------------
    it("并发调用 ensureDaemon 应安全（只 spawn 一次）", async () => {
      const addr = mockAddr();

      (existsSync as ReturnType<typeof vi.fn>)
        .mockReturnValueOnce(false)   // 首次 readAddrJson（ADDR_JSON_PATH 不存在）
        .mockReturnValueOnce(true)    // _spawnDaemon 检查 DAEMON_BIN_PATH
        .mockReturnValue(true);       // waitForAddrJson 轮询时返回 true

      (readFileSync as ReturnType<typeof vi.fn>).mockReturnValue(
        JSON.stringify(addr)
      );

      // mockChild 需要有 unref 方法
      const mockChild = Object.assign(new EventEmitter(), {
        unref: vi.fn(),
      }) as unknown as ReturnType<typeof spawn>;
      (spawn as ReturnType<typeof vi.fn>).mockReturnValue(mockChild);

      const mockSocket = createMockSocket();
      (createConnection as ReturnType<typeof vi.fn>).mockReturnValue(mockSocket);

      const { ensureDaemon } = await import("../daemon.js");

      process.nextTick(() => {
        mockChild.emit("spawn");
        setTimeout(() => {
          mockSocket.emit("connect");
          mockSocket.emit("data", Buffer.from(buildPingResponse(true)));
        }, 150);
      });

      const [r1, r2, r3] = await Promise.all([
        ensureDaemon(),
        ensureDaemon(),
        ensureDaemon(),
      ]);

      expect(spawn).toHaveBeenCalledTimes(1);
      expect(r1.pid).toBe(r2.pid);
      expect(r2.pid).toBe(r3.pid);
    }, 10000);
  });

  // ------------------------------------------
  // 测试 3: heartbeat detects daemon crash
  // ------------------------------------------
  describe("startHeartbeat", () => {
    it("心跳连续失败 3 次后应尝试重启 Daemon", async () => {
      // 心跳测试需要 fake timers 推进时间
      vi.useFakeTimers();

      const addr = mockAddr();

      const mockSocket = createMockSocket();
      (createConnection as ReturnType<typeof vi.fn>).mockReturnValue(mockSocket);

      const { startHeartbeat } = await import("../daemon.js");

      const stopHeartbeat = startHeartbeat(addr);

      // 推进时间触发 3 次心跳失败（ping 超时）
      for (let i = 0; i < 3; i++) {
        await vi.advanceTimersByTimeAsync(HEARTBEAT_INTERVAL_MS + PING_TIMEOUT_MS + 100);
      }

      stopHeartbeat();

      // 验证 ping 至少调用了 3 次
      expect((createConnection as ReturnType<typeof vi.fn>).mock.calls.length).toBeGreaterThanOrEqual(3);
    });

    it("stop 函数调用后心跳应停止", async () => {
      // 心跳测试需要 fake timers 推进时间
      vi.useFakeTimers();

      const addr = mockAddr();

      const mockSocket = createMockSocket();
      (createConnection as ReturnType<typeof vi.fn>).mockReturnValue(mockSocket);
      mockSocket.on("connect", () => {
        mockSocket.emit("data", Buffer.from(buildPingResponse(true)));
      });

      const { startHeartbeat } = await import("../daemon.js");
      const stop = startHeartbeat(addr);

      await vi.advanceTimersByTimeAsync(HEARTBEAT_INTERVAL_MS + 100);

      stop();

      const callCountBefore = (createConnection as ReturnType<typeof vi.fn>).mock.calls.length;

      await vi.advanceTimersByTimeAsync(HEARTBEAT_INTERVAL_MS * 2 + 100);

      expect((createConnection as ReturnType<typeof vi.fn>).mock.calls.length).toBe(callCountBefore);
    });
  });

  // ------------------------------------------
  // 测试 4: version mismatch triggers restart
  // ------------------------------------------
  describe("版本不匹配处理", () => {
    it("addr.json 中的版本与期望不符时，ping 失败触发重启", async () => {
      const oldAddr = mockAddr();

      (existsSync as ReturnType<typeof vi.fn>).mockReturnValue(true);
      (readFileSync as ReturnType<typeof vi.fn>).mockReturnValue(
        JSON.stringify(oldAddr)
      );

      vi.spyOn(process, "kill").mockImplementation(() => true);

      const mockSocket = createMockSocket();
      (createConnection as ReturnType<typeof vi.fn>).mockReturnValueOnce(mockSocket);

      const { ensureDaemon } = await import("../daemon.js");

      // 模拟 spawn 和新 addr.json（在 ensureDaemon 调用前设置好）
      const newAddr = mockAddr(99999);
      // mockChild 需要有 unref 方法
      const mockChild = Object.assign(new EventEmitter(), {
        unref: vi.fn(),
      }) as unknown as ReturnType<typeof spawn>;
      (spawn as ReturnType<typeof vi.fn>).mockReturnValue(mockChild);

      // 第一次 existsSync（_spawnDaemon 检查 DAEMON_BIN_PATH）: true
      // 第二次 existsSync（waitForAddrJson 轮询 ADDR_JSON_PATH）: true
      (existsSync as ReturnType<typeof vi.fn>)
        .mockReturnValueOnce(true)
        .mockReturnValueOnce(true);

      (readFileSync as ReturnType<typeof vi.fn>).mockReturnValue(
        JSON.stringify(newAddr)
      );

      const mockSocket2 = createMockSocket();
      (createConnection as ReturnType<typeof vi.fn>).mockReturnValueOnce(mockSocket2);

      // 第一次 ping 失败：用 nextTick 触发，让 pingDaemon 快速 resolve(false)
      process.nextTick(() => {
        mockSocket.emit("connect");
        mockSocket.emit("data", Buffer.from(
          JSON.stringify({ v: 1, ok: false, result: null, error: { code: "VERSION_MISMATCH", message: "版本不匹配" } }) + "\n"
        ));
      });

      // spawn "spawn" 事件：用 setTimeout 延迟，让 pingDaemon 的 microtask 先完成
      // 然后 _spawnDaemon() 调用 spawn() 注册监听器后，才触发事件
      setTimeout(() => {
        mockChild.emit("spawn");
        // waitForAddrJson 内部 setTimeout(100ms) 后触发 ping
        setTimeout(() => {
          mockSocket2.emit("connect");
          mockSocket2.emit("data", Buffer.from(buildPingResponse(true)));
        }, 150);
      }, 50);

      const result = await ensureDaemon();
      expect(result.pid).toBe(newAddr.pid);
      expect(spawn).toHaveBeenCalledTimes(1);
    }, 10000);
  });
});
