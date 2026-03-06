/**
 * @file stdin-pipe.test.ts
 * @description stdin 管道读取工具函数单元测试（TDD）
 *              测试 readStdinFromStream 函数的以下性质：
 *              1. 空输入（直接 end）返回空字符串
 *              2. 单块 data 输入返回该块内容
 *              3. 多块 data 输入正确拼接
 *              4. 超时（2000ms）内返回已收到数据
 *              5. Promise 仅 resolve 一次（settled 标志防抖）
 *              6. PBT：随机分块输入流，性质 result === chunks.join("")
 * @author Atlas.oi
 * @date 2026-03-05
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { EventEmitter } from "node:events";
import fc from "fast-check";

// ============================================
// 核心逻辑：readStdinFromStream（可注入依赖，便于测试）
// 与 scripts/lib/stdin.mjs 中的 readStdin 实现等价
// readStdin() 本质上是 readStdinFromStream(process.stdin) 的包装
// ============================================

/**
 * 从给定的可读流中读取完整输入，带超时保护和 settled 防抖
 *
 * 业务逻辑：
 * 1. 监听 data 事件，累积所有分块数据
 * 2. 监听 end 事件，安全 resolve
 * 3. 监听 error 事件，安全 reject
 * 4. timeoutMs 后兜底 resolve 已收到的数据
 * 5. settled 标志确保 Promise 只被结算一次
 *
 * @param {NodeJS.ReadableStream} stream - 要读取的输入流
 * @param {number} timeoutMs - 超时毫秒数（默认 2000）
 * @returns {Promise<string>} 流的完整文本内容
 */
function readStdinFromStream(stream: NodeJS.ReadableStream, timeoutMs = 2000): Promise<string> {
  return new Promise((resolve, reject) => {
    // settled 标志：防止 end 事件和超时同时 resolve，导致回调执行两次
    let settled = false;
    let data = "";

    function safeResolve(value: string) {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve(value);
    }

    function safeReject(err: Error) {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      reject(err);
    }

    // 设置 UTF-8 编码（若流支持 setEncoding）
    if (typeof (stream as any).setEncoding === "function") {
      (stream as any).setEncoding("utf-8");
    }

    stream.on("data", (chunk: string | Buffer) => {
      data += typeof chunk === "string" ? chunk : chunk.toString("utf-8");
    });

    stream.on("end", () => {
      safeResolve(data);
    });

    stream.on("error", (err: Error) => {
      safeReject(err);
    });

    // 超时兜底
    const timer = setTimeout(() => {
      safeResolve(data);
    }, timeoutMs);
  });
}

// ============================================
// 辅助：创建模拟的 stdin 流
// ============================================

/**
 * 创建可控的 EventEmitter 模拟流
 */
function createMockStream() {
  const stream = new EventEmitter() as NodeJS.ReadableStream & EventEmitter;
  (stream as any).setEncoding = vi.fn();
  return {
    stream,
    /** 依次发送多个 data 块 */
    sendChunks: (chunks: string[]) => {
      for (const chunk of chunks) {
        stream.emit("data", chunk);
      }
    },
    /** 发送 end 事件，模拟流关闭 */
    sendEnd: () => stream.emit("end"),
    /** 发送 error 事件 */
    sendError: (err: Error) => stream.emit("error", err),
  };
}

// ============================================
// 测试套件：readStdinFromStream 基本行为
// ============================================

describe("readStdinFromStream - 基本行为", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("空输入（直接 end）应返回空字符串", async () => {
    const mock = createMockStream();
    const promise = readStdinFromStream(mock.stream);
    mock.sendEnd();
    const result = await promise;
    expect(result).toBe("");
  });

  it("单块 data 输入应返回该块内容", async () => {
    const mock = createMockStream();
    const promise = readStdinFromStream(mock.stream);
    mock.sendChunks(['{"prompt":"hello"}']);
    mock.sendEnd();
    const result = await promise;
    expect(result).toBe('{"prompt":"hello"}');
  });

  it("多块 data 输入应正确拼接", async () => {
    const mock = createMockStream();
    const promise = readStdinFromStream(mock.stream);
    // 分三块发送 JSON
    mock.sendChunks(['{"pro', 'mpt":', '"world"}']);
    mock.sendEnd();
    const result = await promise;
    expect(result).toBe('{"prompt":"world"}');
  });

  it("超时（2000ms）内即使没有 end 事件，也应返回已收到数据", async () => {
    const mock = createMockStream();
    const promise = readStdinFromStream(mock.stream, 2000);
    // 发送数据但不触发 end 事件
    mock.sendChunks(["partial data"]);
    // 快进时间触发超时
    vi.advanceTimersByTime(2001);
    const result = await promise;
    expect(result).toBe("partial data");
  });

  it("Promise 仅 resolve 一次：end 后再触发超时不应改变结果", async () => {
    const mock = createMockStream();
    let resolveCount = 0;

    const promise = readStdinFromStream(mock.stream, 2000).then((val) => {
      resolveCount++;
      return val;
    });

    mock.sendChunks(["first resolve"]);
    mock.sendEnd();

    // 等待 end 触发的 resolve
    const result = await promise;
    expect(result).toBe("first resolve");
    expect(resolveCount).toBe(1);

    // 快进时间，触发超时（settled 标志应阻止第二次 resolve）
    vi.advanceTimersByTime(2001);
    // 清空 microtask 队列
    await Promise.resolve();
    await Promise.resolve();

    // resolve 次数仍应为 1
    expect(resolveCount).toBe(1);
  });

  it("超时后再触发 end 不应二次 resolve", async () => {
    const mock = createMockStream();
    let resolveCount = 0;

    const promise = readStdinFromStream(mock.stream, 2000).then((val) => {
      resolveCount++;
      return val;
    });

    mock.sendChunks(["data before timeout"]);
    // 先触发超时
    vi.advanceTimersByTime(2001);

    const result = await promise;
    expect(result).toBe("data before timeout");
    expect(resolveCount).toBe(1);

    // 超时后再触发 end，不应引发额外的 resolve
    mock.sendEnd();
    await Promise.resolve();
    await Promise.resolve();

    expect(resolveCount).toBe(1);
  });

  it("setEncoding 应被调用", () => {
    const mock = createMockStream();
    readStdinFromStream(mock.stream);
    // setEncoding 应被调用（异步，立即执行）
    expect((mock.stream as any).setEncoding).toHaveBeenCalledWith("utf-8");
  });
});

// ============================================
// PBT（基于属性的测试）：随机分块流拼接性质
// ============================================

describe("readStdinFromStream - PBT: 随机分块流拼接", () => {
  // 性质：对于任意 chunks 列表，输出等于 chunks.join("")
  it("任意分块输入的拼接结果等于 chunks.join('')", async () => {
    await fc.assert(
      fc.asyncProperty(
        // 生成 0-10 个长度 0-20 的任意 ASCII 字符串数组
        fc.array(fc.string({ maxLength: 20 }), { minLength: 0, maxLength: 10 }),
        async (chunks) => {
          const mock = createMockStream();
          const promise = readStdinFromStream(mock.stream);
          mock.sendChunks(chunks);
          mock.sendEnd();
          const result = await promise;
          expect(result).toBe(chunks.join(""));
        }
      ),
      { numRuns: 50 }
    );
  });
});
