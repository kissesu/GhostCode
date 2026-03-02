# Team Plan: T18 IPC 桥接层

## 概述

实现 `src/plugin/src/ipc.ts`，完成 TypeScript Plugin 与 Rust Daemon 之间的 IPC 通信层。
采用 Node.js `net` 模块连接 Unix socket，协议为换行符分隔的 JSON（newline-delimited JSON）。
维护单持久连接 + 自动重连，单请求超时 10s，p99 延迟目标 < 100ms。

**前置依赖**: T17（Daemon 生命周期管理，提供 `getSocketPath()` 函数和 DaemonStatus 类型）

**产出文件**: `src/plugin/src/ipc.ts`（替换 T16 中的占位文件）

**测试文件**: `src/plugin/src/ipc.test.ts`（vitest）

---

## 参考溯源

- 参考: `cccc/src/cccc/daemon/client_ops.py:11-56` - Unix socket 客户端连接逻辑（connect → sendall(JSON\n) → readline）
- 参考: `cccc/src/cccc/daemon/socket_protocol_ops.py:11-29` - 服务端 recv_json_line / send_json 实现，确认行分隔协议
- 参考: `cccc/src/cccc/daemon/server.py:968-988` - call_daemon 超时参数（timeout_s）、端点读取逻辑
- 参考: `crates/ghostcode-types/src/ipc.rs:21-97` - Rust 侧 DaemonRequest/DaemonResponse/DaemonError 字段定义（v, op, args / v, ok, result, error）
- 参考: `schemas/daemon-request.json` - 请求 Schema（required: [v, op, args]，additionalProperties: false）
- 参考: `schemas/daemon-response.json` - 响应 Schema（required: [v, ok, result]，error 字段含 code/message）
- 参考: `cccc/src/cccc/ports/mcp/common.py:125-147` - _call_daemon_or_raise 模式：resp.ok 为 false 时抛出带结构的错误

---

## 协议规范（与 Rust Daemon 对齐）

### DaemonRequest（发送）
```json
{ "v": 1, "op": "ping", "args": {} }
```
字段来自 `schemas/daemon-request.json` 和 `crates/ghostcode-types/src/ipc.rs:21-28`：
- `v`: 固定为整数 `1`
- `op`: 操作名称字符串
- `args`: 任意 JSON 值（空操作传 `{}`，不传 `null`）

### DaemonResponse（接收）
```json
{ "v": 1, "ok": true, "result": { ... } }
{ "v": 1, "ok": false, "result": null, "error": { "code": "NOT_FOUND", "message": "..." } }
```
字段来自 `schemas/daemon-response.json` 和 `crates/ghostcode-types/src/ipc.rs:51-86`：
- `v`: 固定为整数 `1`
- `ok`: boolean
- `result`: ok=true 时的返回值（Rust 侧 `serde_json::Value`，可为任意 JSON）
- `error`: ok=false 时的错误对象，含 `code` 和 `message` 字段

### 传输协议
- 每条消息 = 一行 JSON + `\n`（换行符）
- 参考: `cccc/src/cccc/daemon/client_ops.py:31,48` - `sendall(json.dumps(payload) + "\n")`
- 参考: `cccc/src/cccc/daemon/client_ops.py:33,50` - `f.readline(4_000_000)` 按行读取

---

## TypeScript 类型定义（与 Schema 完全对齐）

```typescript
// 与 schemas/daemon-request.json 和 crates/ghostcode-types/src/ipc.rs:DaemonRequest 对齐
interface DaemonRequest {
  v: 1;                              // 协议版本，固定为字面量 1
  op: string;                        // 操作名称
  args: Record<string, unknown>;     // 操作参数
}

// 与 schemas/daemon-response.json 和 crates/ghostcode-types/src/ipc.rs:DaemonResponse 对齐
interface DaemonError {
  code: string;    // 错误码（如 "NOT_FOUND", "INVALID_ARGS"）
  message: string; // 可读错误描述
}

interface DaemonResponse {
  v: 1;                       // 协议版本，固定为字面量 1
  ok: boolean;                // 操作是否成功
  result: unknown;            // 成功时返回值（对应 serde_json::Value）
  error?: DaemonError;        // 失败时的错误（skip_serializing_if = Option::is_none）
}
```

**契约测试要点**: `DaemonRequest` 的 `v` 字段必须序列化为整数 `1`，不能为字符串；`args` 不可为 `undefined`，空参数时传 `{}`。

---

## 架构设计

### 连接池策略
- 维护 **1 个持久连接**（`_socket: net.Socket | null`）
- 请求时检查连接状态：若 null 或 destroyed 则先重连
- 重连为同步阻塞当前请求（await 重连完成后再发请求，不排队）
- 断开事件（`close`/`error`）：清空 `_socket`，不立即重连（等下次请求时懒重连）

### 并发安全
- 同一时刻只允许 **1 个 in-flight 请求**（持久连接 + 行协议无请求 ID）
- 用 `_pending` 互斥锁（Promise chain）实现请求串行化
- 若队列中已有请求在等待，新请求在前一个完成后才发送

### 超时机制
- 每次 `callDaemon` 调用绑定 10s 超时（`AbortSignal` + `setTimeout`）
- 超时时：reject with `IpcTimeoutError`，清空 socket（强制断开，下次重连）

### 重连机制
- 最多重试 3 次，每次间隔 100ms（指数退避：100ms, 200ms, 400ms）
- 超过重试次数后抛出 `IpcConnectionError`

---

## 子任务列表

---

### Task 1: 类型定义与错误类（可并行）

- **类型**: 源码
- **文件范围**: `src/plugin/src/ipc.ts` 顶部（类型 + 错误类）
- **依赖**: 无
- **实施步骤**:

  1. 写入文件头注释（严格按规范格式）：
  ```typescript
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
   * @date 2026-03-01
   */
  ```

  2. 导入 `node:net`：
  ```typescript
  import * as net from "node:net";
  ```

  3. 导出协议类型（export，供测试和外部使用）：
  ```typescript
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
    /**
     * 失败时的错误信息。
     * Rust 侧 #[serde(skip_serializing_if = "Option::is_none")]，
     * ok=true 时此字段不存在。
     */
    error?: DaemonError;
  }
  ```

  4. 定义错误类（IpcTimeoutError / IpcConnectionError / IpcProtocolError）：
  ```typescript
  // ============================================
  // 自定义错误类型
  // 区分超时、连接失败、协议错误三种场景
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
  ```

- **验收标准**: 类型可被外部模块导入，`tsc --noEmit` 零错误

---

### Task 2: createConnection 函数（可并行）

- **类型**: 源码
- **文件范围**: `src/plugin/src/ipc.ts`（createConnection 函数）
- **依赖**: Task 1（需要 IpcConnectionError 错误类）
- **实施步骤**:

  实现 `createConnection(socketPath: string): Promise<net.Socket>` 函数，对应公共 API 要求。

  完整实现逻辑：
  1. 调用 `net.createConnection({ path: socketPath })`，返回 Socket 实例
  2. 监听 `connect` 事件 → resolve Promise，返回已连接的 Socket
  3. 监听 `error` 事件 → reject Promise，抛出 `IpcConnectionError`
  4. 设置 Socket 选项：`setEncoding("utf8")`、`setKeepAlive(true, 5000)`

  ```typescript
  /**
   * 创建到 Daemon Unix socket 的原始连接
   *
   * 业务逻辑：
   * 1. 使用 Node.js net.createConnection 连接指定 Unix socket 路径
   * 2. 等待 connect 事件确认连接建立
   * 3. 连接失败时抛出 IpcConnectionError（含 socket 路径和原因）
   * 4. 成功后设置 keepAlive 保持连接活跃
   *
   * 参考: cccc/src/cccc/daemon/client_ops.py:40-55 - AF_UNIX connect 逻辑
   *
   * @param socketPath - Unix socket 文件路径（如 /tmp/ghostcode.sock）
   * @returns 已建立连接的 net.Socket 实例
   * @throws {IpcConnectionError} 无法连接时抛出
   */
  export function createConnection(socketPath: string): Promise<net.Socket> {
    return new Promise((resolve, reject) => {
      // 创建 Unix socket 连接
      const sock = net.createConnection({ path: socketPath });

      // 连接成功：配置编码和保活，然后 resolve
      sock.once("connect", () => {
        // UTF-8 文本编码，与协议 JSON 序列化保持一致
        sock.setEncoding("utf8");
        // 每 5 秒发送一次 TCP keepalive 探测，防止空闲连接被系统切断
        sock.setKeepAlive(true, 5000);
        resolve(sock);
      });

      // 连接失败：reject，连接由 Node.js 自动关闭
      sock.once("error", (err) => {
        reject(new IpcConnectionError(socketPath, err));
      });
    });
  }
  ```

- **验收标准**: 函数签名与任务规格一致（`Promise<net.Socket>`），TypeScript 类型正确

---

### Task 3: IpcClient 类实现（依赖 Task 1 + Task 2）

- **类型**: 源码（核心业务逻辑）
- **文件范围**: `src/plugin/src/ipc.ts`（IpcClient 类）
- **依赖**: Task 1（类型 + 错误类），Task 2（createConnection）
- **实施步骤**:

  实现管理持久连接 + 串行化请求 + 自动重连的 `IpcClient` 类。

  #### 3.1 类结构与字段

  ```typescript
  // 单次请求的超时时间（毫秒）
  // 参考: T18 任务规格 "超时：单次请求 10s"
  const REQUEST_TIMEOUT_MS = 10_000;

  // 重连最大次数
  const MAX_RECONNECT_ATTEMPTS = 3;

  // 重连基础间隔（毫秒），指数退避：100ms → 200ms → 400ms
  const RECONNECT_BASE_DELAY_MS = 100;

  /**
   * IPC 客户端
   *
   * 维护到 Daemon Unix socket 的单持久连接。
   * 设计约束：
   * - 同一时刻只有 1 个 in-flight 请求（行协议无请求 ID，必须串行）
   * - 请求串行化通过 Promise chain（_pending）实现
   * - 断开后懒重连（下次请求触发重连，不主动维护心跳）
   */
  class IpcClient {
    // Unix socket 文件路径，由 T17 getDaemonSocketPath() 提供
    private readonly _socketPath: string;

    // 当前活跃的 socket 连接，null 表示未连接
    private _socket: net.Socket | null = null;

    // 请求串行化锁：_pending 始终指向最后一个请求的 Promise
    // 新请求通过 .then() 链接在 _pending 之后，确保顺序执行
    private _pending: Promise<void> = Promise.resolve();

    // 行缓冲区：未完整行的数据暂存于此，等待 \n 到来
    private _lineBuffer: string = "";

    // 当前等待响应的 resolve/reject 回调
    // 每次请求开始时设置，收到响应行后清空
    private _responseResolver: ((line: string) => void) | null = null;
    private _responseRejector: ((err: Error) => void) | null = null;

    constructor(socketPath: string) {
      this._socketPath = socketPath;
    }
  ```

  #### 3.2 连接建立与重连逻辑

  ```typescript
    /**
     * 确保 socket 连接可用，必要时重连
     *
     * 业务逻辑：
     * 1. 检查当前 socket 是否存活（!destroyed && !destroyed）
     * 2. 若已连接则直接返回
     * 3. 若未连接则尝试重连，最多 MAX_RECONNECT_ATTEMPTS 次
     * 4. 每次失败后等待指数退避时间
     * 5. 全部失败后抛出 IpcConnectionError
     *
     * @throws {IpcConnectionError} 所有重试均失败时抛出
     */
    private async _ensureConnected(): Promise<void> {
      // 检查现有连接是否仍然有效
      if (this._socket !== null && !this._socket.destroyed) {
        return;
      }

      // 清空旧连接状态
      this._socket = null;
      this._lineBuffer = "";

      // 指数退避重连循环
      let lastError: Error | undefined;
      for (let attempt = 0; attempt < MAX_RECONNECT_ATTEMPTS; attempt++) {
        if (attempt > 0) {
          // 指数退避：100ms → 200ms → 400ms
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
     *
     * 业务逻辑：
     * 1. 监听 data 事件：追加到行缓冲区，检测 \n 后触发响应 resolver
     * 2. 监听 close/error 事件：清空 socket，若有等待中的 resolver 则 reject
     *
     * 参考: cccc/src/cccc/daemon/socket_protocol_ops.py:11-24 - recv_json_line 行读取逻辑
     *
     * @param sock - 已建立连接的 Socket 实例
     */
    private _setupSocket(sock: net.Socket): void {
      this._socket = sock;
      this._lineBuffer = "";

      // 数据到达：追加缓冲区，寻找换行符
      sock.on("data", (chunk: string) => {
        // chunk 为 string（因为 setEncoding("utf8") 已在 createConnection 中设置）
        this._lineBuffer += chunk;

        // 提取所有完整的行（可能一次收到多行）
        const newlineIndex = this._lineBuffer.indexOf("\n");
        if (newlineIndex !== -1) {
          // 截取第一行（Rust Daemon 每次只发送一行响应）
          const line = this._lineBuffer.slice(0, newlineIndex);
          this._lineBuffer = this._lineBuffer.slice(newlineIndex + 1);

          // 通知等待中的请求
          if (this._responseResolver !== null) {
            const resolve = this._responseResolver;
            this._responseResolver = null;
            this._responseRejector = null;
            resolve(line);
          }
        }
      });

      // 连接关闭：清空连接，reject 任何等待中的请求
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
  ```

  #### 3.3 请求发送逻辑

  ```typescript
    /**
     * 发送请求到 Daemon 并等待响应（串行化执行）
     *
     * 业务逻辑：
     * 1. 将请求追加到 _pending Promise chain（串行化保证）
     * 2. 确保连接可用（懒重连）
     * 3. 序列化请求为 JSON + \n 并发送
     * 4. 等待 data 事件触发 _responseResolver，同时设置 10s 超时
     * 5. 超时时强制断开 socket（清除 _socket），下次请求会重连
     * 6. 解析响应 JSON，验证 ok 字段
     *
     * 参考: cccc/src/cccc/daemon/client_ops.py:31,48 - sendall(json.dumps() + "\n")
     * 参考: cccc/src/cccc/daemon/client_ops.py:33,50 - readline()
     * 参考: cccc/src/cccc/daemon/server.py:968-988 - call_daemon 超时 timeout_s
     *
     * @param op - 操作名称
     * @param args - 操作参数（默认为 {}）
     * @returns Daemon 响应
     * @throws {IpcTimeoutError} 超过 10s 未收到响应
     * @throws {IpcConnectionError} 连接断开或重连失败
     * @throws {IpcProtocolError} 收到无效 JSON 或响应结构不合法
     */
    async send(op: string, args: Record<string, unknown> = {}): Promise<DaemonResponse> {
      // 串行化：将此请求追加到 _pending 链
      const result = this._pending.then(() => this._doSend(op, args));
      // 更新 _pending（不 catch，错误通过 result 传播给调用者）
      this._pending = result.then(() => undefined, () => undefined);
      return result;
    }

    /**
     * 实际执行单次请求（在串行化保证下调用）
     *
     * @param op - 操作名称
     * @param args - 操作参数
     */
    private async _doSend(
      op: string,
      args: Record<string, unknown>
    ): Promise<DaemonResponse> {
      // ============================================
      // 第一步：确保连接可用（懒重连）
      // ============================================
      await this._ensureConnected();

      const sock = this._socket!;

      // ============================================
      // 第二步：构造请求 payload，序列化为 JSON + \n
      // v 字段固定为 1（与 Rust DaemonRequest.new() 一致）
      // ============================================
      const request: DaemonRequest = { v: 1, op, args };
      const payload = JSON.stringify(request) + "\n";

      // ============================================
      // 第三步：发送请求，等待响应行（带超时）
      // ============================================
      const responseLine = await new Promise<string>((resolve, reject) => {
        // 设置响应 resolver（由 _setupSocket 中的 data 监听器触发）
        this._responseResolver = resolve;
        this._responseRejector = reject;

        // 发送 JSON 行到 Daemon
        sock.write(payload, "utf8");

        // 10s 超时：清空 socket 并 reject
        const timer = setTimeout(() => {
          // 强制断开：下次请求将重连
          if (this._socket !== null) {
            this._socket.destroy();
            this._socket = null;
          }
          this._lineBuffer = "";
          if (this._responseRejector !== null) {
            this._responseResolver = null;
            this._responseRejector = null;
            reject(new IpcTimeoutError(op, REQUEST_TIMEOUT_MS));
          }
        }, REQUEST_TIMEOUT_MS);

        // 收到响应后清除超时计时器
        // 通过包装 resolve 实现
        const originalResolver = this._responseResolver;
        this._responseResolver = (line: string) => {
          clearTimeout(timer);
          originalResolver(line);
        };
      });

      // ============================================
      // 第四步：解析响应 JSON，验证结构
      // ============================================
      let parsed: unknown;
      try {
        parsed = JSON.parse(responseLine);
      } catch {
        throw new IpcProtocolError(
          `响应不是合法 JSON: "${responseLine.slice(0, 100)}"`
        );
      }

      // 验证必填字段（与 schemas/daemon-response.json 的 required: [v, ok, result] 对齐）
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
     *
     * 优雅关闭：先等待所有 pending 请求完成，再销毁 socket。
     */
    async close(): Promise<void> {
      // 等待所有已排队的请求完成
      await this._pending;
      if (this._socket !== null) {
        this._socket.destroy();
        this._socket = null;
      }
    }
  }
  ```

- **验收标准**: 单持久连接 + 串行化 + 超时 + 重连逻辑均完整实现，`tsc --noEmit` 零错误

---

### Task 4: 公共 API 函数（依赖 Task 3）

- **类型**: 源码
- **文件范围**: `src/plugin/src/ipc.ts`（模块级单例 + 公共 API）
- **依赖**: Task 3（IpcClient 类）
- **实施步骤**:

  实现模块级单例连接管理和公共 API `callDaemon`。

  ```typescript
  // ============================================
  // 模块级单例 IpcClient
  // 整个 Plugin 进程共享一个 IpcClient 实例（一条连接）
  // ============================================

  // 当前的单例 IpcClient，null 表示尚未初始化
  // 通过 getClient() 懒初始化
  let _client: IpcClient | null = null;

  // 当前使用的 socket 路径，由 callDaemon 传入或从 T17 getDaemonSocketPath() 获取
  let _socketPath: string = "";

  /**
   * 获取（或创建）模块级 IpcClient 单例
   *
   * 业务逻辑：
   * 1. 若 _client 已存在且 socketPath 未变，直接返回
   * 2. 若 socketPath 变化（Daemon 重启），重建 IpcClient
   * 3. socketPath 为空时抛出 IpcConnectionError
   *
   * @param socketPath - Unix socket 文件路径
   */
  function _getClient(socketPath: string): IpcClient {
    if (!socketPath) {
      throw new IpcConnectionError("<empty>", new Error("socketPath 不能为空"));
    }
    // socketPath 变化时（如 Daemon 重启后 socket 路径改变）重建客户端
    if (_client === null || _socketPath !== socketPath) {
      if (_client !== null) {
        // 销毁旧连接（不等待，fire-and-forget）
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
   * 业务逻辑：
   * 1. 获取模块级 IpcClient 单例（懒初始化）
   * 2. 调用 client.send(op, args) 完成完整的请求-响应周期
   * 3. 错误透传给调用方（不在此处 catch）
   *
   * 参考: cccc/src/cccc/daemon/server.py:968-988 - call_daemon() 函数设计
   * 参考: cccc/src/cccc/ports/mcp/common.py:125-147 - _call_daemon_or_raise 模式
   *
   * @param op - 操作名称（如 "ping"、"actor.add"）
   * @param args - 操作参数（默认为 {}，不传 null）
   * @param socketPath - Unix socket 路径（可选，默认从环境变量 GHOSTCODE_SOCKET_PATH 读取）
   * @returns Daemon 响应
   * @throws {IpcTimeoutError} 超时
   * @throws {IpcConnectionError} 连接失败
   * @throws {IpcProtocolError} 协议错误
   *
   * @example
   * const resp = await callDaemon("ping");
   * if (resp.ok) console.log("Daemon 存活");
   *
   * @example
   * const resp = await callDaemon("actor.add", { actor_id: "peer-1" }, "/tmp/ghostcode.sock");
   */
  export async function callDaemon(
    op: string,
    args: Record<string, unknown> = {},
    socketPath?: string
  ): Promise<DaemonResponse> {
    // 解析 socket 路径：优先使用参数，其次读取环境变量
    const resolvedPath = socketPath
      ?? process.env["GHOSTCODE_SOCKET_PATH"]
      ?? "";

    const client = _getClient(resolvedPath);
    return client.send(op, args);
  }

  /**
   * 重置模块级单例（主要用于测试场景）
   *
   * 关闭现有连接并清空单例，下次 callDaemon 时重新创建。
   */
  export async function resetClient(): Promise<void> {
    if (_client !== null) {
      await _client.close();
      _client = null;
      _socketPath = "";
    }
  }
  ```

  **注意**: `callDaemon` 的签名简化为 3 个参数（op, args, socketPath?）而非规格中的 `(op, args?)` 形式，原因是 socketPath 不应每次调用都传入（单例模式）。规格要求的 `callDaemon(op, args?)` 签名通过可选的 `args` 参数满足。

  **与规格对齐**: 规格中 `callDaemon(op: string, args?: Record<string, unknown>): Promise<DaemonResponse>`，本实现多了可选的 `socketPath` 参数，向下兼容，不破坏规格约定。

- **验收标准**: `callDaemon("ping")` 可直接调用（args 为可选参数），`createConnection` 和 `callDaemon` 均为 export

---

### Task 5: 测试文件（可与 Task 3 并行开始，依赖 Task 4 完成后补全）

- **类型**: 测试
- **文件范围**: `src/plugin/src/ipc.test.ts`（新建）
- **依赖**: Task 1-4 全部完成（测试需要完整实现）
- **实施步骤**:

  #### 5.1 文件头与导入

  ```typescript
  /**
   * @file IPC 桥接层单元测试
   * @description ipc.ts 的 vitest 测试套件，覆盖 TDD 要求的 4 个场景。
   *              使用 Node.js net.createServer 创建真实 Unix socket 服务端进行集成测试。
   * @author Atlas.oi
   * @date 2026-03-01
   */
  import { describe, it, expect, beforeEach, afterEach } from "vitest";
  import * as net from "node:net";
  import * as os from "node:os";
  import * as path from "node:path";
  import * as fs from "node:fs";
  import {
    callDaemon,
    createConnection,
    resetClient,
    type DaemonResponse,
    IpcTimeoutError,
    IpcConnectionError,
  } from "./ipc.js";
  ```

  #### 5.2 测试辅助工具（Mock Daemon Server）

  ```typescript
  // ============================================
  // Mock Daemon Server 工具
  // 在测试中创建真实 Unix socket 服务端
  // ============================================

  /**
   * 创建一个简单的 Mock Daemon Server
   *
   * @param socketPath - Unix socket 路径
   * @param handler - 接收请求 JSON 对象，返回响应 JSON 对象
   * @returns { server, close } - server 实例和关闭函数
   */
  function createMockDaemon(
    socketPath: string,
    handler: (req: unknown) => unknown
  ): { server: net.Server; close: () => Promise<void> } {
    // 确保旧 socket 文件不存在
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

  /** 生成临时 socket 路径（每次测试独立） */
  function tmpSocket(): string {
    return path.join(os.tmpdir(), `ghostcode-test-${Date.now()}-${Math.random().toString(36).slice(2)}.sock`);
  }
  ```

  #### 5.3 测试用例 1: callDaemon ping 返回 ok

  ```typescript
  describe("callDaemon", () => {
    let socketPath: string;
    let closeMockDaemon: () => Promise<void>;

    beforeEach(async () => {
      await resetClient();
      socketPath = tmpSocket();
    });

    afterEach(async () => {
      await resetClient();
      if (closeMockDaemon) await closeMockDaemon();
    });

    it("callDaemon ping 返回 ok", async () => {
      // 创建 Mock Daemon：收到 ping 请求返回 { v:1, ok:true, result: { pong: true } }
      const { close } = createMockDaemon(socketPath, (req) => {
        const r = req as { op: string };
        if (r.op === "ping") {
          return { v: 1, ok: true, result: { pong: true } };
        }
        return { v: 1, ok: false, result: null, error: { code: "UNKNOWN_OP", message: "unknown op" } };
      });
      closeMockDaemon = close;

      // 等待服务端启动
      await new Promise<void>((r) => setTimeout(r, 50));

      const resp = await callDaemon("ping", {}, socketPath);

      expect(resp.ok).toBe(true);
      expect(resp.v).toBe(1);
      expect(resp.result).toMatchObject({ pong: true });
    });
  ```

  #### 5.4 测试用例 2: callDaemon 10s 超时

  ```typescript
    it("callDaemon 超时 after 10s", async () => {
      // Mock Daemon：收到请求后永远不回复（模拟超时场景）
      const { close } = createMockDaemon(socketPath, (_req) => {
        // 返回一个永远不 resolve 的 Promise（不写入响应）
        return new Promise(() => { /* 永不 resolve */ });
      });
      closeMockDaemon = close;

      // 等待服务端启动
      await new Promise<void>((r) => setTimeout(r, 50));

      // 临时将超时时间改为 100ms 测试（通过 vi.useFakeTimers 或修改常量）
      // 注意：vitest 使用真实超时时，此测试需等待 10s，实际测试中应 mock setTimeout
      // 此处使用 vi.useFakeTimers 推进时间
      // ---- 实现方案：使用真实 timer 但缩短超时（通过环境变量注入）----
      // 实际测试文件中需 import { vi } from "vitest" 并 vi.useFakeTimers()

      await expect(callDaemon("ping", {}, socketPath)).rejects.toThrow(IpcTimeoutError);
    }, 15_000); // 设置测试超时 15s（真实 timer 情况）
  ```

  **实现说明**: `callDaemon` 超时测试有两种实现方案：
  - 方案 A（推荐）: 使用 `vi.useFakeTimers()` + `vi.advanceTimersByTime(10001)` 推进时间，无需真实等待 10s
  - 方案 B: 在 `ipc.ts` 中通过 `process.env.IPC_TIMEOUT_MS_OVERRIDE` 允许测试注入缩短的超时值（仅 test 环境）

  计划文件采用方案 A（vitest 官方推荐），具体代码如下：
  ```typescript
  // 在 beforeEach 中: vi.useFakeTimers()
  // 在 afterEach 中: vi.useRealTimers()
  // 测试中: const promise = callDaemon("ping", {}, socketPath);
  //         vi.advanceTimersByTime(10001);
  //         await expect(promise).rejects.toThrow(IpcTimeoutError);
  ```

  #### 5.5 测试用例 3: 自动重连

  ```typescript
    it("断开后自动重连", async () => {
      let connectionCount = 0;

      // Mock Daemon：第 1 次连接立即断开，第 2 次正常响应
      const server = net.createServer((conn) => {
        connectionCount++;
        if (connectionCount === 1) {
          // 第 1 次连接：立即关闭
          conn.destroy();
          return;
        }
        // 第 2 次连接：正常响应
        conn.setEncoding("utf8");
        let buf = "";
        conn.on("data", (chunk: string) => {
          buf += chunk;
          if (buf.includes("\n")) {
            conn.write(JSON.stringify({ v: 1, ok: true, result: { pong: true } }) + "\n");
          }
        });
      });

      try { fs.unlinkSync(socketPath); } catch { /* 忽略 */ }
      server.listen(socketPath);
      closeMockDaemon = () => new Promise((r) => server.close(() => r()));

      await new Promise<void>((r) => setTimeout(r, 50));

      // 第 1 次请求：连接成功后 Mock 立即断开，IpcClient 捕获断开并重连
      // 注意：此测试测的是"重连后的第 2 次请求成功"
      // 因为 IpcClient 采用懒重连，第 1 次连接断开后 socket 被清空
      // 第 2 次调用 callDaemon 时触发重连，连接到第 2 次 server 处理
      try {
        await callDaemon("ping", {}, socketPath);
      } catch {
        // 第 1 次可能失败（连接被服务端断开），继续测试重连
      }

      // 重置客户端模拟断开后重连
      // （懒重连在下次 callDaemon 时触发）
      const resp = await callDaemon("ping", {}, socketPath);
      expect(resp.ok).toBe(true);
      expect(connectionCount).toBeGreaterThanOrEqual(2);
    });
  ```

  #### 5.6 测试用例 4: p99 延迟 < 100ms（1000 次 ping）

  ```typescript
    it("p99 延迟 < 100ms（1000 次 ping）", async () => {
      // Mock Daemon：立即返回 ping 响应（模拟零计算延迟）
      const { close } = createMockDaemon(socketPath, () => ({
        v: 1,
        ok: true,
        result: { pong: true },
      }));
      closeMockDaemon = close;

      await new Promise<void>((r) => setTimeout(r, 50));

      // 预热：建立连接
      await callDaemon("ping", {}, socketPath);

      // 执行 1000 次串行 ping，记录每次延迟
      const latencies: number[] = [];
      for (let i = 0; i < 1000; i++) {
        const start = performance.now();
        await callDaemon("ping", {}, socketPath);
        latencies.push(performance.now() - start);
      }

      // 计算 p99（取第 990 大的延迟）
      latencies.sort((a, b) => a - b);
      const p99 = latencies[Math.floor(latencies.length * 0.99)]!;

      expect(p99).toBeLessThan(100);
    }, 60_000); // 1000 次串行请求，设置 60s 超时
  ```

  #### 5.7 测试用例 5: 契约测试（类型对齐验证）

  ```typescript
  describe("契约测试：DaemonRequest/Response 与 Schema 对齐", () => {
    it("发送的请求包含必填字段 v=1, op, args", async () => {
      const socketPath2 = tmpSocket();
      let receivedRequest: unknown = null;

      const { close } = createMockDaemon(socketPath2, (req) => {
        receivedRequest = req;
        return { v: 1, ok: true, result: null };
      });

      await new Promise<void>((r) => setTimeout(r, 50));
      await callDaemon("test.op", { key: "value" }, socketPath2);
      await close();

      // 验证与 schemas/daemon-request.json 对齐：required: [v, op, args]
      expect(receivedRequest).toMatchObject({
        v: 1,          // 整数 1，不是字符串 "1"
        op: "test.op", // 操作名称
        args: { key: "value" }, // 操作参数
      });

      // 验证 additionalProperties: false（无额外字段）
      const keys = Object.keys(receivedRequest as object);
      expect(keys).toEqual(expect.arrayContaining(["v", "op", "args"]));
      expect(keys.length).toBe(3);
    });

    it("空参数时 args 为 {} 而非 null 或 undefined", async () => {
      const socketPath3 = tmpSocket();
      let receivedRequest: unknown = null;

      const { close } = createMockDaemon(socketPath3, (req) => {
        receivedRequest = req;
        return { v: 1, ok: true, result: null };
      });

      await new Promise<void>((r) => setTimeout(r, 50));
      await callDaemon("ping", undefined, socketPath3); // args 未传
      await close();

      expect((receivedRequest as { args: unknown }).args).toStrictEqual({});
    });
  });
  ```

- **验收标准**: 5 个测试用例全部通过，`pnpm test` 零失败

---

### Task 6: vitest 配置（可并行，如果 T16 未配置）

- **类型**: 配置
- **文件范围**: `src/plugin/vitest.config.ts`（新建）
- **依赖**: Task 1（需要确认 package.json 已有 vitest 依赖）
- **实施步骤**:

  1. 检查 `src/plugin/package.json` 是否已有 vitest devDependency，如没有则添加：
  ```json
  "vitest": "^2.0.0"
  ```

  2. 在 package.json scripts 中添加 test 命令：
  ```json
  "test": "vitest run",
  "test:watch": "vitest"
  ```

  3. 创建 `src/plugin/vitest.config.ts`：
  ```typescript
  /**
   * @file vitest 测试配置
   * @description GhostCode Plugin 的 vitest 测试框架配置
   * @author Atlas.oi
   * @date 2026-03-01
   */
  import { defineConfig } from "vitest/config";

  export default defineConfig({
    test: {
      // 使用 Node.js 运行环境（IPC 测试需要 net 模块）
      environment: "node",
      // 全局 API（describe/it/expect）无需导入
      globals: false,
      // 测试文件匹配模式
      include: ["src/**/*.test.ts"],
      // 超时：默认 5s，p99 测试中个别用例会覆盖
      testTimeout: 5_000,
    },
  });
  ```

- **验收标准**: `pnpm test` 命令可以正常运行

---

## 文件冲突检查

| Task | 文件 | 操作 |
|------|------|------|
| Task 1-4 | `src/plugin/src/ipc.ts` | 覆写（替换 T16 占位） |
| Task 5 | `src/plugin/src/ipc.test.ts` | 新建 |
| Task 6 | `src/plugin/vitest.config.ts` | 新建 |
| Task 6 | `src/plugin/package.json` | 修改（添加 vitest 依赖和 test 脚本） |

**注意**: `src/plugin/src/ipc.ts` 在 T16 中已创建为占位文件，T18 将完整覆写其内容。

---

## TDD 强制执行规范

本任务必须严格遵循 TDD 流程：Red → Green → Refactor。

```
Red    → 先写测试文件（Task 5: ipc.test.ts）+ vitest 配置（Task 6）+ ipc.ts 最小 stub
Green  → 补全 ipc.ts 完整实现（Task 1-4），让所有测试通过
Refactor → 验证 pnpm build + pnpm test 全部通过
```

---

## 并行分组

```
Layer 1（TDD Red — 测试先行，可并行）:
  Task 5: 测试文件 __tests__/ipc.test.ts（完整测试用例）
  Task 6: vitest 配置
  + 创建 ipc.ts 最小 stub（仅类型定义 + 函数签名，body 抛异常）
  验证: pnpm test 编译通过但测试失败（Red）

Layer 2（TDD Green — 实现让测试通过）:
  Task 1: 类型定义 + 错误类（替换 stub 中对应部分）
  Task 2: createConnection 函数
  Task 3: IpcClient 类实现
  Task 4: 公共 API（callDaemon + resetClient）
  验证: pnpm test 所有测试通过（Green）

Layer 3（TDD Refactor — 验证）:
  pnpm build + pnpm test 最终确认
```

**推荐执行顺序**:
```
Layer 1: Task 5 + Task 6 (Red: 测试先写 + ipc.ts stub)
             ↓
Layer 2: Task 1 → 2 → 3 → 4 (Green: ipc.ts 完整实现)
             ↓
Layer 3: 最终验证 (Refactor)
```

---

## 潜在问题及解决方案

| 问题 | 原因 | 解决方案 |
|------|------|---------|
| `ENOENT: no such file or directory` 连接失败 | socket 文件不存在（Daemon 未启动） | 重连失败后抛出 IpcConnectionError，错误信息含 socket 路径 |
| 响应解析失败：`SyntaxError: Unexpected token` | Daemon 发送了非 JSON 内容 | 捕获 JSON.parse 异常，抛出 IpcProtocolError |
| 串行化破坏：两个请求同时发送 | _pending 链未正确更新 | 确保 _pending = result.then(noop, noop)，不使用 catch（catch 会创建新链） |
| `setEncoding` 与 `data` 事件的 chunk 类型 | setEncoding 后 chunk 为 string，否则为 Buffer | createConnection 中调用 `sock.setEncoding("utf8")`，测试中确认类型 |
| p99 测试在 CI 环境超过 100ms | Unix socket 通信在低性能 CI 机器上延迟较高 | 若 CI 环境不稳定，可在 vitest.config.ts 中通过 `process.env.CI` 跳过此测试 |
| 超时测试需真实等待 10s | 默认使用真实 timer | 使用 `vi.useFakeTimers()` + `vi.advanceTimersByTime(10001)` 推进时间 |

---

## Builder 注意事项

1. **ESM 导入路径**: `src/index.ts` 中导入 `ipc.ts` 时使用 `"./ipc.js"`（带 .js 扩展名），与 T16 保持一致

2. **T16 占位文件覆写**: T16 的 `src/plugin/src/ipc.ts` 定义了不同的接口（IpcRequest/IpcResponse/IpcClient），T18 将完整覆写，同时更新 `src/plugin/src/index.ts` 中的导出

3. **index.ts 需同步更新**: T18 完成后，`src/plugin/src/index.ts` 中的 ipc 相关导出需从旧接口更新为新接口（DaemonRequest/DaemonResponse/callDaemon/createConnection/resetClient）

4. **T17 依赖**: 若 T17 已实现 `getSocketPath(): string`，可在 callDaemon 的默认 socketPath 解析中调用它；当前实现通过 `process.env.GHOSTCODE_SOCKET_PATH` 降级兼容，无需硬依赖 T17

5. **args 默认值**: `callDaemon` 中 `args` 参数缺省值为 `{}`，而非 `undefined` 或 `null`，因为 Rust Daemon 的 `DaemonRequest.args` 为 `serde_json::Value`，空对象序列化为 `{}` 最安全
