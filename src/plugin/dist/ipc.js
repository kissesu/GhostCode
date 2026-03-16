import * as net from "node:net";
import { homedir } from "node:os";
import { readFileSync } from "node:fs";
import { StreamingHandler } from "./router/streaming";
import { join } from "node:path";
class IpcTimeoutError extends Error {
  constructor(op, timeoutMs) {
    super(`IPC \u8BF7\u6C42\u8D85\u65F6: op="${op}" \u8D85\u8FC7 ${timeoutMs}ms \u672A\u6536\u5230\u54CD\u5E94`);
    this.name = "IpcTimeoutError";
  }
}
class IpcConnectionError extends Error {
  constructor(socketPath, cause) {
    super(
      `IPC \u8FDE\u63A5\u5931\u8D25: socket="${socketPath}"` + (cause ? ` \u539F\u56E0: ${cause.message}` : "")
    );
    this.name = "IpcConnectionError";
    if (cause) this.cause = cause;
  }
}
class IpcProtocolError extends Error {
  constructor(detail) {
    super(`IPC \u534F\u8BAE\u9519\u8BEF: ${detail}`);
    this.name = "IpcProtocolError";
  }
}
const REQUEST_TIMEOUT_MS = 1e4;
const MAX_RECONNECT_ATTEMPTS = 3;
const RECONNECT_BASE_DELAY_MS = 100;
function createConnection(socketPath) {
  return new Promise((resolve, reject) => {
    const sock = net.createConnection({ path: socketPath });
    sock.once("connect", () => {
      sock.setEncoding("utf8");
      sock.setKeepAlive(true, 5e3);
      resolve(sock);
    });
    sock.once("error", (err) => {
      reject(new IpcConnectionError(socketPath, err));
    });
  });
}
class IpcClient {
  _socketPath;
  _socket = null;
  _pending = Promise.resolve();
  _lineBuffer = "";
  _responseResolver = null;
  _responseRejector = null;
  constructor(socketPath) {
    this._socketPath = socketPath;
  }
  /**
   * 确保 socket 连接可用，必要时重连
   */
  async _ensureConnected() {
    if (this._socket !== null && !this._socket.destroyed) {
      return;
    }
    this._socket = null;
    this._lineBuffer = "";
    let lastError;
    for (let attempt = 0; attempt < MAX_RECONNECT_ATTEMPTS; attempt++) {
      if (attempt > 0) {
        const delay = RECONNECT_BASE_DELAY_MS * Math.pow(2, attempt - 1);
        await new Promise((r) => setTimeout(r, delay));
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
  _setupSocket(sock) {
    this._socket = sock;
    this._lineBuffer = "";
    sock.on("data", (chunk) => {
      this._lineBuffer += chunk;
      if (this._lineBuffer.length > 4 * 1024 * 1024) {
        this._socket?.destroy();
        return;
      }
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
    const onDisconnect = (err) => {
      this._socket = null;
      this._lineBuffer = "";
      if (this._responseRejector !== null) {
        const reject = this._responseRejector;
        this._responseResolver = null;
        this._responseRejector = null;
        reject(
          new IpcConnectionError(
            this._socketPath,
            err ?? new Error("\u8FDE\u63A5\u610F\u5916\u65AD\u5F00")
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
  async send(op, args = {}) {
    const result = this._pending.then(() => this._doSend(op, args));
    this._pending = result.then(() => void 0, () => void 0);
    return result;
  }
  /**
   * 实际执行单次请求
   */
  async _doSend(op, args) {
    await this._ensureConnected();
    const sock = this._socket;
    const request = { v: 1, op, args };
    const payload = JSON.stringify(request) + "\n";
    const responseLine = await new Promise((resolve, reject) => {
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
      this._responseResolver = (line) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        resolve(line);
      };
      this._responseRejector = (err) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        reject(err);
      };
      sock.write(payload, "utf8");
    });
    let parsed;
    try {
      parsed = JSON.parse(responseLine);
    } catch {
      throw new IpcProtocolError(
        `\u54CD\u5E94\u4E0D\u662F\u5408\u6CD5 JSON: "${responseLine.slice(0, 100)}"`
      );
    }
    if (typeof parsed !== "object" || parsed === null || !("v" in parsed) || !("ok" in parsed) || !("result" in parsed)) {
      throw new IpcProtocolError(
        `\u54CD\u5E94\u7F3A\u5C11\u5FC5\u586B\u5B57\u6BB5 (v, ok, result): ${JSON.stringify(parsed).slice(0, 100)}`
      );
    }
    return parsed;
  }
  /**
   * 关闭 IPC 连接
   */
  async close() {
    await this._pending;
    if (this._socket !== null) {
      this._socket.destroy();
      this._socket = null;
    }
  }
  /**
   * 发送流式请求到 Daemon，持续读取多行 JSON 事件直到流结束
   *
   * 业务逻辑说明：
   * 1. 串行化执行（同一时刻只有 1 个 in-flight 请求）
   * 2. 发送请求后持续监听 socket data 事件
   * 3. 每收到完整行，传给 StreamingHandler.handleLine() 解析并分发回调
   * 4. StreamingHandler.isComplete() 为 true 时结束监听，返回 StreamResponse
   * 5. 超时（REQUEST_TIMEOUT_MS）时终止并抛出 IpcTimeoutError
   *
   * @param op - 操作名称
   * @param args - 操作参数
   * @param callbacks - 流式事件回调集合
   * @returns StreamResponse 包含所有收到的事件和锁定的 sessionId
   */
  async sendStream(op, args, callbacks) {
    const result = this._pending.then(
      () => this._doSendStream(op, args, callbacks)
    );
    this._pending = result.then(() => void 0, () => void 0);
    return result;
  }
  /**
   * 实际执行流式请求
   */
  async _doSendStream(op, args, callbacks) {
    await this._ensureConnected();
    const sock = this._socket;
    const request = { v: 1, op, args };
    const payload = JSON.stringify(request) + "\n";
    const handler = new StreamingHandler(callbacks);
    const collectedEvents = [];
    const wrappedCallbacks = {
      onInit: (ev) => {
        collectedEvents.push(ev);
        callbacks.onInit?.(ev);
      },
      onProgress: (ev) => {
        collectedEvents.push(ev);
        callbacks.onProgress?.(ev);
      },
      onAgentMessage: (ev) => {
        collectedEvents.push(ev);
        callbacks.onAgentMessage?.(ev);
      },
      onComplete: (ev) => {
        collectedEvents.push(ev);
        callbacks.onComplete?.(ev);
      },
      onError: (ev) => {
        collectedEvents.push(ev);
        callbacks.onError?.(ev);
      }
    };
    const collectHandler = new StreamingHandler(wrappedCallbacks);
    return new Promise((resolve, reject) => {
      let settled = false;
      let streamBuffer = "";
      const timer = setTimeout(() => {
        if (settled) return;
        settled = true;
        cleanup();
        if (this._socket !== null) {
          this._socket.destroy();
          this._socket = null;
        }
        this._lineBuffer = "";
        reject(new IpcTimeoutError(op, REQUEST_TIMEOUT_MS));
      }, REQUEST_TIMEOUT_MS);
      const onData = (chunk) => {
        streamBuffer += chunk;
        if (streamBuffer.length > 4 * 1024 * 1024) {
          if (settled) return;
          settled = true;
          cleanup();
          if (this._socket !== null) {
            this._socket.destroy();
            this._socket = null;
          }
          this._lineBuffer = "";
          reject(new IpcProtocolError("\u6D41\u5F0F\u54CD\u5E94\u7F13\u51B2\u533A\u8D85\u8FC7 4MB \u4E0A\u9650"));
          return;
        }
        let newlineIndex;
        while ((newlineIndex = streamBuffer.indexOf("\n")) !== -1) {
          const line = streamBuffer.slice(0, newlineIndex);
          streamBuffer = streamBuffer.slice(newlineIndex + 1);
          try {
            const parsed = JSON.parse(line);
            if (typeof parsed === "object" && parsed !== null && "v" in parsed && "ok" in parsed && "result" in parsed) {
              if (settled) return;
              settled = true;
              cleanup();
              resolve({
                events: collectedEvents,
                sessionId: collectHandler.getSessionId()
              });
              return;
            }
          } catch {
          }
          collectHandler.handleLine(line);
          if (collectHandler.isComplete()) {
            if (settled) return;
            settled = true;
            cleanup();
            resolve({
              events: collectedEvents,
              sessionId: collectHandler.getSessionId()
            });
            return;
          }
        }
      };
      const onDisconnect = (err) => {
        if (settled) return;
        settled = true;
        cleanup();
        this._socket = null;
        this._lineBuffer = "";
        reject(
          new IpcConnectionError(
            this._socketPath,
            err ?? new Error("\u6D41\u5F0F\u54CD\u5E94\u671F\u95F4\u8FDE\u63A5\u610F\u5916\u65AD\u5F00")
          )
        );
      };
      const cleanup = () => {
        clearTimeout(timer);
        sock.removeListener("data", onData);
        sock.removeListener("close", onCloseHandler);
        sock.removeListener("error", onErrorHandler);
        this._lineBuffer = "";
      };
      const onCloseHandler = () => onDisconnect();
      const onErrorHandler = (err) => onDisconnect(err);
      sock.removeAllListeners("data");
      sock.on("data", onData);
      sock.once("close", onCloseHandler);
      sock.once("error", onErrorHandler);
      sock.write(payload, "utf8");
    });
  }
}
let _client = null;
let _socketPath = "";
function _getClient(socketPath) {
  if (!socketPath) {
    throw new IpcConnectionError("<empty>", new Error("socketPath \u4E0D\u80FD\u4E3A\u7A7A"));
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
const ADDR_JSON_PATH = join(homedir(), ".ghostcode", "daemon", "ghostcoded.addr.json");
function resolveSocketPath(explicit) {
  if (explicit) return explicit;
  const envPath = process.env["GHOSTCODE_SOCKET_PATH"];
  if (envPath) return envPath;
  try {
    const content = readFileSync(ADDR_JSON_PATH, "utf-8");
    const parsed = JSON.parse(content);
    if (parsed?.path && typeof parsed.path === "string") {
      return parsed.path;
    }
  } catch {
  }
  return null;
}
async function callDaemon(op, args = {}, socketPath) {
  const resolvedPath = resolveSocketPath(socketPath);
  if (!resolvedPath) {
    throw new IpcConnectionError(
      "<\u672A\u627E\u5230>",
      new Error("\u65E0\u6CD5\u89E3\u6790 Socket \u8DEF\u5F84\u3002\u8BF7\u786E\u4FDD Daemon \u5DF2\u542F\u52A8\u6216\u8BBE\u7F6E GHOSTCODE_SOCKET_PATH \u73AF\u5883\u53D8\u91CF")
    );
  }
  const client = _getClient(resolvedPath);
  return client.send(op, args);
}
async function resetClient() {
  if (_client !== null) {
    await _client.close();
    _client = null;
    _socketPath = "";
  }
}
async function callDaemonStream(op, args = {}, callbacks, socketPath) {
  const resolvedPath = resolveSocketPath(socketPath);
  if (!resolvedPath) {
    throw new IpcConnectionError(
      "<\u672A\u627E\u5230>",
      new Error("\u65E0\u6CD5\u89E3\u6790 Socket \u8DEF\u5F84\uFF08\u6D41\u5F0F\u8C03\u7528\uFF09\u3002\u8BF7\u786E\u4FDD Daemon \u5DF2\u542F\u52A8\u6216\u8BBE\u7F6E GHOSTCODE_SOCKET_PATH \u73AF\u5883\u53D8\u91CF")
    );
  }
  const client = _getClient(resolvedPath);
  return client.sendStream(op, args, callbacks);
}
export {
  IpcConnectionError,
  IpcProtocolError,
  IpcTimeoutError,
  callDaemon,
  callDaemonStream,
  createConnection,
  resetClient,
  resolveSocketPath
};
//# sourceMappingURL=ipc.js.map