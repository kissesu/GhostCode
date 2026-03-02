/**
 * @file IPC 桥接层单元测试
 * @description ipc.ts 的 vitest 测试套件，覆盖 TDD 要求的场景。
 *              使用 Node.js net.createServer 创建真实 Unix socket 服务端进行集成测试。
 * @author Atlas.oi
 * @date 2026-03-02
 */
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as net from "node:net";
import * as os from "node:os";
import * as path from "node:path";
import * as fs from "node:fs";
import {
  callDaemon,
  createConnection as createIpcConnection,
  resetClient,
  type DaemonResponse,
  IpcTimeoutError,
  IpcConnectionError,
} from "./ipc.js";

// ============================================
// Mock Daemon Server 工具
// ============================================

/**
 * 创建一个简单的 Mock Daemon Server
 */
function createMockDaemon(
  socketPath: string,
  handler: (req: unknown) => unknown
): { server: net.Server; close: () => Promise<void> } {
  try { fs.unlinkSync(socketPath); } catch { /* 忽略 */ }

  const server = net.createServer((conn) => {
    conn.setEncoding("utf8");
    let buf = "";
    conn.on("data", (chunk: string) => {
      buf += chunk;
      const newlineIdx = buf.indexOf("\n");
      if (newlineIdx !== -1) {
        const line = buf.slice(0, newlineIdx);
        buf = buf.slice(newlineIdx + 1);
        try {
          const req = JSON.parse(line);
          const resp = handler(req);
          // 如果 handler 返回 Promise（模拟超时），不写响应
          if (resp && typeof resp === "object" && typeof (resp as { then?: unknown }).then === "function") {
            return; // 永不响应，用于超时测试
          }
          conn.write(JSON.stringify(resp) + "\n", "utf8");
        } catch {
          conn.write(
            JSON.stringify({ v: 1, ok: false, result: null, error: { code: "PARSE_ERROR", message: "parse error" } }) + "\n",
            "utf8"
          );
        }
      }
    });
  });

  server.listen(socketPath);

  const close = (): Promise<void> =>
    new Promise((resolve) => {
      server.close(() => {
        try { fs.unlinkSync(socketPath); } catch { /* 忽略 */ }
        resolve();
      });
    });

  return { server, close };
}

/** 生成临时 socket 路径 */
function tmpSocket(): string {
  return path.join(os.tmpdir(), `ghostcode-test-${Date.now()}-${Math.random().toString(36).slice(2)}.sock`);
}

// ============================================
// 测试套件
// ============================================

describe("callDaemon", () => {
  let socketPath: string;
  let closeMockDaemon: (() => Promise<void>) | null = null;

  beforeEach(async () => {
    await resetClient();
    socketPath = tmpSocket();
  });

  afterEach(async () => {
    await resetClient();
    if (closeMockDaemon) {
      await closeMockDaemon();
      closeMockDaemon = null;
    }
  });

  it("callDaemon ping 返回 ok", async () => {
    const { close } = createMockDaemon(socketPath, (req) => {
      const r = req as { op: string };
      if (r.op === "ping") {
        return { v: 1, ok: true, result: { pong: true } };
      }
      return { v: 1, ok: false, result: null, error: { code: "UNKNOWN_OP", message: "unknown op" } };
    });
    closeMockDaemon = close;

    await new Promise<void>((r) => setTimeout(r, 50));

    const resp = await callDaemon("ping", {}, socketPath);

    expect(resp.ok).toBe(true);
    expect(resp.v).toBe(1);
    expect(resp.result).toMatchObject({ pong: true });
  });

  it("callDaemon 超时 after 10s", async () => {
    // Mock Daemon：收到请求后永远不回复
    const { close } = createMockDaemon(socketPath, () => {
      return new Promise(() => { /* 永不 resolve */ });
    });
    closeMockDaemon = close;

    await new Promise<void>((r) => setTimeout(r, 50));

    await expect(callDaemon("ping", {}, socketPath)).rejects.toThrow(IpcTimeoutError);
  }, 15_000);

  it("断开后自动重连", async () => {
    let connectionCount = 0;

    const server = net.createServer((conn) => {
      connectionCount++;
      if (connectionCount === 1) {
        // 第 1 次连接：立即关闭
        conn.destroy();
        return;
      }
      // 后续连接：正常响应
      conn.setEncoding("utf8");
      let buf = "";
      conn.on("data", (chunk: string) => {
        buf += chunk;
        if (buf.includes("\n")) {
          conn.write(JSON.stringify({ v: 1, ok: true, result: { pong: true } }) + "\n");
          buf = "";
        }
      });
    });

    try { fs.unlinkSync(socketPath); } catch { /* 忽略 */ }
    server.listen(socketPath);
    closeMockDaemon = () => new Promise((r) => server.close(() => {
      try { fs.unlinkSync(socketPath); } catch { /* 忽略 */ }
      r();
    }));

    await new Promise<void>((r) => setTimeout(r, 50));

    // 第 1 次请求可能失败（连接被服务端断开）
    try {
      await callDaemon("ping", {}, socketPath);
    } catch {
      // 预期失败
    }

    // 第 2 次请求触发重连
    const resp = await callDaemon("ping", {}, socketPath);
    expect(resp.ok).toBe(true);
    expect(connectionCount).toBeGreaterThanOrEqual(2);
  });

  it("p99 延迟 < 100ms（1000 次 ping）", async () => {
    const { close } = createMockDaemon(socketPath, () => ({
      v: 1,
      ok: true,
      result: { pong: true },
    }));
    closeMockDaemon = close;

    await new Promise<void>((r) => setTimeout(r, 50));

    // 预热
    await callDaemon("ping", {}, socketPath);

    const latencies: number[] = [];
    for (let i = 0; i < 1000; i++) {
      const start = performance.now();
      await callDaemon("ping", {}, socketPath);
      latencies.push(performance.now() - start);
    }

    latencies.sort((a, b) => a - b);
    const p99 = latencies[Math.floor(latencies.length * 0.99)]!;

    expect(p99).toBeLessThan(100);
  }, 60_000);
});

describe("契约测试：DaemonRequest/Response 与 Schema 对齐", () => {
  let closeMockDaemon: (() => Promise<void>) | null = null;

  afterEach(async () => {
    await resetClient();
    if (closeMockDaemon) {
      await closeMockDaemon();
      closeMockDaemon = null;
    }
  });

  it("发送的请求包含必填字段 v=1, op, args", async () => {
    const socketPath = tmpSocket();
    let receivedRequest: unknown = null;

    const { close } = createMockDaemon(socketPath, (req) => {
      receivedRequest = req;
      return { v: 1, ok: true, result: null };
    });
    closeMockDaemon = close;

    await new Promise<void>((r) => setTimeout(r, 50));
    await callDaemon("test.op", { key: "value" }, socketPath);

    expect(receivedRequest).toMatchObject({
      v: 1,
      op: "test.op",
      args: { key: "value" },
    });

    const keys = Object.keys(receivedRequest as object);
    expect(keys).toEqual(expect.arrayContaining(["v", "op", "args"]));
    expect(keys.length).toBe(3);
  });

  it("空参数时 args 为 {} 而非 null 或 undefined", async () => {
    const socketPath = tmpSocket();
    let receivedRequest: unknown = null;

    const { close } = createMockDaemon(socketPath, (req) => {
      receivedRequest = req;
      return { v: 1, ok: true, result: null };
    });
    closeMockDaemon = close;

    await new Promise<void>((r) => setTimeout(r, 50));
    await callDaemon("ping", undefined as unknown as Record<string, unknown>, socketPath);

    expect((receivedRequest as { args: unknown }).args).toStrictEqual({});
  });
});
