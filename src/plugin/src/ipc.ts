/**
 * @file IPC 桥接层
 * @description GhostCode Plugin 与 Rust Daemon 之间的 Unix socket IPC 通信层。
 *              协议：换行符分隔的 JSON（newline-delimited JSON），每条消息为一行 JSON + \n。
 *              连接策略：维护 1 个持久连接，断开后懒重连（下次请求时自动重连）。
 *              超时：单次请求 10s，重连最多 3 次，指数退避（100ms / 200ms / 400ms）。
 *
 *              类型设计与 Rust 侧 DaemonRequest/DaemonResponse 完全对齐：
 *              参考: crates/ghostcode-types/src/ipc.rs
 *              参考: schemas/daemon-request.json, schemas/daemon-response.json
 * @author Atlas.oi
 * @date 2026-03-02
 */

import * as net from "node:net";

// ============================================
// 协议类型定义
// 与 Rust 侧 crates/ghostcode-types/src/ipc.rs 对齐
// 与 schemas/daemon-request.json 对齐：required: [v, op, args]
// ============================================

/** Daemon 请求结构 - 与 DaemonRequest Rust struct 字段一一对应 */
export interface DaemonRequest {
  /** 协议版本，固定为整数 1（与 Rust v: u8 = 1 对应） */
  v: 1;
  /** 操作名称，如 "ping"、"actor.add" */
  op: string;
  /** 操作参数，任意 JSON 对象（空参数时传 {}，不传 null） */
  args: Record<string, unknown>;
}

/** Daemon 错误结构 - 与 DaemonError Rust struct 字段一一对应 */
export interface DaemonError {
  /** 错误码，如 "NOT_FOUND"、"INVALID_ARGS" */
  code: string;
  /** 可读的错误描述 */
  message: string;
}

/** Daemon 响应结构 - 与 DaemonResponse Rust struct 字段一一对应 */
export interface DaemonResponse {
  /** 协议版本，固定为整数 1 */
  v: 1;
  /** 操作是否成功 */
  ok: boolean;
  /** 成功时的返回值（对应 Rust serde_json::Value，可为任意 JSON） */
  result: unknown;
  /** 失败时的错误信息 */
  error?: DaemonError;
}

// ============================================
// 自定义错误类型
// ============================================

/** IPC 请求超时（单次请求超过 10s 未收到响应） */
export class IpcTimeoutError extends Error {
  constructor(op: string, timeoutMs: number) {
    super(`IPC 请求超时: op="${op}" 超过 ${timeoutMs}ms 未收到响应`);
    this.name = "IpcTimeoutError";
  }
}

/** IPC 连接失败（无法建立或恢复 Unix socket 连接） */
export class IpcConnectionError extends Error {
  constructor(socketPath: string, cause?: Error) {
    super(
      `IPC 连接失败: socket="${socketPath}"` +
      (cause ? ` 原因: ${cause.message}` : "")
    );
    this.name = "IpcConnectionError";
    if (cause) this.cause = cause;
  }
}

/** IPC 协议错误（收到无效 JSON 或响应结构不符合 Schema） */
export class IpcProtocolError extends Error {
  constructor(detail: string) {
    super(`IPC 协议错误: ${detail}`);
    this.name = "IpcProtocolError";
  }
}

// ============================================
// 常量
// ============================================

/** 单次请求的超时时间（毫秒） */
const REQUEST_TIMEOUT_MS = 10_000;

/** 重连最大次数 */
const MAX_RECONNECT_ATTEMPTS = 3;

/** 重连基础间隔（毫秒），指数退避：100ms -> 200ms -> 400ms */
const RECONNECT_BASE_DELAY_MS = 100;

// ============================================
// createConnection 函数
// ============================================

/**
 * 创建到 Daemon Unix socket 的原始连接
 *
 * 参考: cccc/src/cccc/daemon/client_ops.py:40-55 - AF_UNIX connect 逻辑
 *
 * @param socketPath - Unix socket 文件路径
 * @returns 已建立连接的 net.Socket 实例
 * @throws {IpcConnectionError} 无法连接时抛出
 */
export function createConnection(socketPath: string): Promise<net.Socket> {
  return new Promise((resolve, reject) => {
    const sock = net.createConnection({ path: socketPath });

    sock.once("connect", () => {
      sock.setEncoding("utf8");
      sock.setKeepAlive(true, 5000);
      resolve(sock);
    });

    sock.once("error", (err) => {
      reject(new IpcConnectionError(socketPath, err));
    });
  });
}

// ============================================
// IpcClient 类
// ============================================

/**
 * IPC 客户端
 *
 * 维护到 Daemon Unix socket 的单持久连接。
 * 同一时刻只有 1 个 in-flight 请求（行协议无请求 ID，必须串行）。
 * 请求串行化通过 Promise chain（_pending）实现。
 * 断开后懒重连（下次请求触发重连）。
 */
class IpcClient {
  private readonly _socketPath: string;
  private _socket: net.Socket | null = null;
  private _pending: Promise<void> = Promise.resolve();
  private _lineBuffer: string = "";
  private _responseResolver: ((line: string) => void) | null = null;
  private _responseRejector: ((err: Error) => void) | null = null;

  constructor(socketPath: string) {
    this._socketPath = socketPath;
  }

  /**
   * 确保 socket 连接可用，必要时重连
   */
  private async _ensureConnected(): Promise<void> {
    if (this._socket !== null && !this._socket.destroyed) {
      return;
    }

    this._socket = null;
    this._lineBuffer = "";

    let lastError: Error | undefined;
    for (let attempt = 0; attempt < MAX_RECONNECT_ATTEMPTS; attempt++) {
      if (attempt > 0) {
        const delay = RECONNECT_BASE_DELAY_MS * Math.pow(2, attempt - 1);
        await new Promise<void>((r) => setTimeout(r, delay));
      }

      try {
        const sock = await createConnection(this._socketPath);
        this._setupSocket(sock);
        return;
      } catch (err) {
        lastError = err instanceof Error ? err : new Error(String(err));
      }
    }

    throw new IpcConnectionError(this._socketPath, lastError);
  }

  /**
   * 配置 socket 事件监听器
   */
  private _setupSocket(sock: net.Socket): void {
    this._socket = sock;
    this._lineBuffer = "";

    sock.on("data", (chunk: string) => {
      this._lineBuffer += chunk;

      // 防止缓冲区无限增长（4MB 上限）
      if (this._lineBuffer.length > 4 * 1024 * 1024) {
        this._socket?.destroy();
        return;
      }

      // 处理缓冲区中的完整行
      const newlineIndex = this._lineBuffer.indexOf("\n");
      if (newlineIndex !== -1) {
        const line = this._lineBuffer.slice(0, newlineIndex);
        this._lineBuffer = this._lineBuffer.slice(newlineIndex + 1);

        if (this._responseResolver !== null) {
          const resolve = this._responseResolver;
          this._responseResolver = null;
          this._responseRejector = null;
          resolve(line);
        }
      }
    });

    const onDisconnect = (err?: Error) => {
      this._socket = null;
      this._lineBuffer = "";
      if (this._responseRejector !== null) {
        const reject = this._responseRejector;
        this._responseResolver = null;
        this._responseRejector = null;
        reject(
          new IpcConnectionError(
            this._socketPath,
            err ?? new Error("连接意外断开")
          )
        );
      }
    };

    sock.once("close", () => onDisconnect());
    sock.once("error", (err) => onDisconnect(err));
  }

  /**
   * 发送请求到 Daemon 并等待响应（串行化执行）
   */
  async send(op: string, args: Record<string, unknown> = {}): Promise<DaemonResponse> {
    const result = this._pending.then(() => this._doSend(op, args));
    this._pending = result.then(() => undefined, () => undefined);
    return result;
  }

  /**
   * 实际执行单次请求
   */
  private async _doSend(
    op: string,
    args: Record<string, unknown>
  ): Promise<DaemonResponse> {
    await this._ensureConnected();

    const sock = this._socket!;
    const request: DaemonRequest = { v: 1, op, args };
    const payload = JSON.stringify(request) + "\n";

    const responseLine = await new Promise<string>((resolve, reject) => {
      // 使用 settled 标志防止超时与响应双重触发
      let settled = false;

      const timer = setTimeout(() => {
        if (settled) return;
        settled = true;
        if (this._socket !== null) {
          this._socket.destroy();
          this._socket = null;
        }
        this._lineBuffer = "";
        this._responseResolver = null;
        this._responseRejector = null;
        reject(new IpcTimeoutError(op, REQUEST_TIMEOUT_MS));
      }, REQUEST_TIMEOUT_MS);

      // 包装 resolve/reject，确保清除 timer 且只触发一次
      this._responseResolver = (line: string) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        resolve(line);
      };
      this._responseRejector = (err: Error) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        reject(err);
      };

      sock.write(payload, "utf8");
    });

    let parsed: unknown;
    try {
      parsed = JSON.parse(responseLine);
    } catch {
      throw new IpcProtocolError(
        `响应不是合法 JSON: "${responseLine.slice(0, 100)}"`
      );
    }

    if (
      typeof parsed !== "object" ||
      parsed === null ||
      !("v" in parsed) ||
      !("ok" in parsed) ||
      !("result" in parsed)
    ) {
      throw new IpcProtocolError(
        `响应缺少必填字段 (v, ok, result): ${JSON.stringify(parsed).slice(0, 100)}`
      );
    }

    return parsed as DaemonResponse;
  }

  /**
   * 关闭 IPC 连接
   */
  async close(): Promise<void> {
    await this._pending;
    if (this._socket !== null) {
      this._socket.destroy();
      this._socket = null;
    }
  }
}

// ============================================
// 模块级单例
// ============================================

let _client: IpcClient | null = null;
let _socketPath: string = "";

function _getClient(socketPath: string): IpcClient {
  if (!socketPath) {
    throw new IpcConnectionError("<empty>", new Error("socketPath 不能为空"));
  }
  if (_client === null || _socketPath !== socketPath) {
    if (_client !== null) {
      void _client.close();
    }
    _client = new IpcClient(socketPath);
    _socketPath = socketPath;
  }
  return _client;
}

/**
 * 向 Daemon 发起一次 RPC 调用（公共 API）
 *
 * 参考: cccc/src/cccc/daemon/server.py:968-988 - call_daemon() 函数设计
 *
 * @param op - 操作名称
 * @param args - 操作参数（默认为 {}）
 * @param socketPath - Unix socket 路径
 * @returns Daemon 响应
 */
export async function callDaemon(
  op: string,
  args: Record<string, unknown> = {},
  socketPath?: string
): Promise<DaemonResponse> {
  const resolvedPath = socketPath
    ?? process.env["GHOSTCODE_SOCKET_PATH"]
    ?? "";

  const client = _getClient(resolvedPath);
  return client.send(op, args);
}

/**
 * 重置模块级单例（主要用于测试场景）
 */
export async function resetClient(): Promise<void> {
  if (_client !== null) {
    await _client.close();
    _client = null;
    _socketPath = "";
  }
}
