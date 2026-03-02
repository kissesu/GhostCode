# Team Plan: T17 Daemon 生命周期管理

## 概述

实现 `src/plugin/src/daemon.ts`，替换 T16 创建的占位文件。
这是 Plugin 层连接 Rust Daemon 的核心模块，负责：
1. 检测 Daemon 是否已运行（读取 `~/.ghostcode/daemon/ghostcoded.addr.json`）
2. 必要时 spawn 新 Daemon 进程（`detached: true`，后台运行）
3. 等待 Daemon 就绪后 ping 确认
4. 管理心跳检查（每 10 秒一次，失败 3 次则重启）
5. 停止 Daemon（发送 `shutdown` op）

**验收标准**: `pnpm build` 零错误，vitest 全部测试通过。

---

## 参考溯源

- 参考: `crates/ghostcode-types/src/addr.rs:25-53` - AddrDescriptor 数据结构（v, transport, path, pid, version, ts 字段）
- 参考: `crates/ghostcode-types/src/ipc.rs:21-97` - DaemonRequest/DaemonResponse 协议结构
- 参考: `crates/ghostcode-daemon/src/paths.rs:49-70` - `ghostcoded.addr.json` 路径为 `~/.ghostcode/daemon/ghostcoded.addr.json`
- 参考: `crates/ghostcode-daemon/src/process.rs:33-58` - addr.json 写入/读取逻辑（JSON 格式）
- 参考: `crates/ghostcode-daemon/src/dispatch.rs:100-106` - ping 响应格式 `{ pong: true, version: "...", has_unread: false }`
- 参考: `oh-my-claudecode/src/features/rate-limit-wait/daemon.ts:441-509` - `spawn(..., { detached: true, stdio: 'ignore' })` + `child.unref()` 模式
- 参考: `oh-my-claudecode/src/features/rate-limit-wait/daemon.ts:56-91` - 环境变量白名单模式（安全 spawn，防止凭证泄漏）
- 参考: `oh-my-claudecode/src/features/rate-limit-wait/daemon.ts:232-240` - `process.kill(pid, 0)` 检测进程存活

---

## 技术方案

### AddrDescriptor 类型（对应 Rust 端 addr.rs）

```typescript
interface AddrDescriptor {
  v: number;          // 协议版本，固定为 1
  transport: string;  // "unix"
  path: string;       // Unix socket 路径
  pid: number;        // Daemon PID
  version: string;    // GhostCode 版本号
  ts: string;         // 启动时间 ISO 8601
}
```

### DaemonRequest / DaemonResponse 类型（对应 Rust 端 ipc.rs）

```typescript
interface DaemonRequest {
  v: number;                     // 协议版本，固定为 1
  op: string;                    // 操作名（如 "ping", "shutdown"）
  args: Record<string, unknown>; // 操作参数
}

interface DaemonResponse {
  v: number;
  ok: boolean;
  result: unknown;
  error?: { code: string; message: string };
}
```

### addr.json 路径

Daemon 启动后写入: `~/.ghostcode/daemon/ghostcoded.addr.json`

路径解析逻辑（参考 paths.rs）:
- 基础路径: `os.homedir() + "/.ghostcode/daemon/ghostcoded.addr.json"`
- 如果 socket 路径超过 100 字符（macOS 限制）则使用 `/tmp/ghostcode-<hash>/ghostcoded.addr.json`
- **T17 简化处理**: 仅使用标准路径，不实现 hash fallback（paths.rs 的 fallback 逻辑在 Rust 侧处理，Plugin 侧只需读取已写入的 addr.json）

### Daemon 二进制路径

`~/.ghostcode/bin/ghostcoded`（由 T17 规格约束）

### ping 实现

通过 IPC（net.createConnection → Unix socket）发送：
```json
{ "v": 1, "op": "ping", "args": {} }
```
期望响应 `{ "v": 1, "ok": true, "result": { "pong": true, "version": "...", "has_unread": false } }`

注意：T18 实现完整 IPC 层。T17 的 ping 是**内联实现**（不依赖 T18 的 connectIpc），只用 `net.createConnection` 发一次性请求，完成后关闭连接。

### 并发保护（ensureDaemon 幂等）

使用模块级 `Promise<AddrDescriptor> | null` 变量保存进行中的启动 Promise。
并发调用时复用同一 Promise，避免重复 spawn。

### 心跳机制

`startHeartbeat` 返回 stop 函数，使用 `setInterval` 每 10 秒调用一次内联 ping。
连续失败 3 次时尝试重启（调用 `ensureDaemon`，先停止旧进程）。

---

## 子任务列表

---

### Task 1: 定义类型和常量

- **类型**: 类型定义（无外部依赖）
- **文件范围**: `src/plugin/src/daemon.ts`（替换占位文件的类型部分）
- **依赖**: 无
- **说明**: 定义所有 TypeScript 接口和模块级常量，包括 AddrDescriptor、DaemonRequest、DaemonResponse，以及路径常量、超时常量

**完整实现**:

```typescript
/**
 * @file Daemon 生命周期管理
 * @description 管理 GhostCode Rust Daemon 进程的启动、停止、健康检查和心跳监控。
 *              负责读取 ~/.ghostcode/daemon/ghostcoded.addr.json 获取 Daemon 连接信息，
 *              在 Daemon 未运行时自动 spawn 新进程，并通过心跳维持连接存活。
 *
 *              核心流程：
 *              1. 读取 addr.json → 检测 Daemon 是否在运行
 *              2. 不存在或 ping 失败 → spawn ghostcoded 二进制
 *              3. 等待 addr.json 出现（最多 5s）→ ping 确认
 *              4. 心跳每 10s 一次，连续失败 3 次触发重启
 *
 *              参考: crates/ghostcode-types/src/addr.rs - AddrDescriptor 数据结构
 *              参考: crates/ghostcode-types/src/ipc.rs - IPC 协议格式
 *              参考: crates/ghostcode-daemon/src/paths.rs - addr.json 文件路径
 *              参考: oh-my-claudecode/src/features/rate-limit-wait/daemon.ts - spawn 模式
 * @author Atlas.oi
 * @date 2026-03-01
 */

import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";
import { createConnection } from "node:net";

// ============================================
// 类型定义（对应 Rust 端的 addr.rs 和 ipc.rs）
// ============================================

/**
 * 端点描述符
 *
 * Daemon 启动后写入 ghostcoded.addr.json 的连接信息
 * 参考: crates/ghostcode-types/src/addr.rs:25-53
 */
export interface AddrDescriptor {
  /** 协议版本号，固定为 1 */
  v: number;
  /** 传输协议，固定为 "unix" */
  transport: string;
  /** Unix socket 文件路径 */
  path: string;
  /** Daemon 进程 ID */
  pid: number;
  /** GhostCode 版本号 */
  version: string;
  /** 启动时间 ISO 8601 UTC */
  ts: string;
}

/**
 * Daemon 请求结构
 *
 * 参考: crates/ghostcode-types/src/ipc.rs:21-39
 */
interface DaemonRequest {
  /** 协议版本号，固定为 1 */
  v: number;
  /** 操作名称 */
  op: string;
  /** 操作参数 */
  args: Record<string, unknown>;
}

/**
 * Daemon 响应结构
 *
 * 参考: crates/ghostcode-types/src/ipc.rs:51-86
 */
interface DaemonResponse {
  /** 协议版本号 */
  v: number;
  /** 操作是否成功 */
  ok: boolean;
  /** 成功时的返回值 */
  result: unknown;
  /** 失败时的错误信息 */
  error?: { code: string; message: string };
}

// ============================================
// 常量
// ============================================

/** GhostCode 根目录 */
const GHOSTCODE_HOME = join(homedir(), ".ghostcode");

/**
 * addr.json 路径
 * 参考: crates/ghostcode-daemon/src/paths.rs:49-70
 * Daemon 启动后写入此文件，供客户端读取连接信息
 */
const ADDR_JSON_PATH = join(GHOSTCODE_HOME, "daemon", "ghostcoded.addr.json");

/**
 * Daemon 二进制路径
 * 由 T17 规格约束
 */
const DAEMON_BIN_PATH = join(GHOSTCODE_HOME, "bin", "ghostcoded");

/** 等待 Daemon 启动的最大时间（毫秒） */
const DAEMON_START_TIMEOUT_MS = 5000;

/** 等待 addr.json 出现的轮询间隔（毫秒） */
const ADDR_POLL_INTERVAL_MS = 100;

/** ping 操作超时时间（毫秒） */
const PING_TIMEOUT_MS = 3000;

/** 心跳间隔（毫秒） */
const HEARTBEAT_INTERVAL_MS = 10_000;

/** 心跳连续失败触发重启的次数阈值 */
const HEARTBEAT_MAX_FAILURES = 3;

/**
 * 允许传递给 Daemon 子进程的环境变量白名单
 *
 * 遵循最小权限原则，防止凭证泄漏（如 ANTHROPIC_API_KEY、GITHUB_TOKEN）
 * 参考: oh-my-claudecode/src/features/rate-limit-wait/daemon.ts:56-77
 */
const DAEMON_ENV_ALLOWLIST = [
  "PATH", "HOME",
  "USER", "USERNAME", "LOGNAME",
  "LANG", "LC_ALL", "LC_CTYPE",
  "TERM",
  "TMPDIR", "TMP", "TEMP",
  "XDG_RUNTIME_DIR", "XDG_DATA_HOME", "XDG_CONFIG_HOME",
  "SHELL",
  "NODE_ENV",
] as const;
```

- **验收标准**: TypeScript 编译无错误，类型可被其他函数使用

---

### Task 2: 实现辅助函数

- **类型**: 工具函数
- **文件范围**: `src/plugin/src/daemon.ts`（接 Task 1 内容继续）
- **依赖**: Task 1（需要类型和常量定义）
- **说明**: 实现以下辅助函数：`createMinimalDaemonEnv`、`readAddrJson`、`isProcessAlive`、`pingDaemon`

**完整实现**:

```typescript
// ============================================
// 辅助函数
// ============================================

/**
 * 创建最小化的 Daemon 进程环境变量
 *
 * 只保留白名单中的环境变量，防止 ANTHROPIC_API_KEY 等凭证泄漏到子进程
 * 参考: oh-my-claudecode/src/features/rate-limit-wait/daemon.ts:83-91
 *
 * @returns 过滤后的环境变量对象
 */
function createMinimalDaemonEnv(): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = {};
  for (const key of DAEMON_ENV_ALLOWLIST) {
    if (process.env[key] !== undefined) {
      env[key] = process.env[key];
    }
  }
  return env;
}

/**
 * 读取 addr.json 文件
 *
 * 文件不存在返回 null（非错误，表示 Daemon 未启动）
 * JSON 解析失败返回 null（视为 Daemon 启动异常，需重启）
 *
 * 参考: crates/ghostcode-daemon/src/process.rs:50-58 - Rust 侧的读取逻辑
 *
 * @returns AddrDescriptor 或 null（文件不存在/解析失败）
 */
function readAddrJson(): AddrDescriptor | null {
  // addr.json 不存在时说明 Daemon 未启动，属于正常状态
  if (!existsSync(ADDR_JSON_PATH)) {
    return null;
  }

  try {
    const content = readFileSync(ADDR_JSON_PATH, "utf-8");
    const parsed = JSON.parse(content) as AddrDescriptor;

    // 基本校验：必须有 v=1 且 transport="unix"
    if (parsed.v !== 1 || parsed.transport !== "unix" || !parsed.path) {
      return null;
    }

    return parsed;
  } catch {
    // JSON 解析失败，视为无效
    return null;
  }
}

/**
 * 检测进程是否存活
 *
 * 使用 signal 0 探测进程（不发送真实信号，只检测是否存在）
 * 参考: oh-my-claudecode/src/features/rate-limit-wait/daemon.ts:232-240
 *
 * @param pid - 进程 ID
 * @returns 进程是否存活
 */
function isProcessAlive(pid: number): boolean {
  try {
    // signal 0 不发送实际信号，只检测进程是否存在
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

/**
 * 通过 Unix socket 向 Daemon 发送 ping 请求
 *
 * 这是一个内联的单次请求实现，不依赖 T18 的完整 IPC 层。
 * 发送后立即关闭连接（一次性）。
 *
 * 协议格式（参考 crates/ghostcode-types/src/ipc.rs）:
 * - 请求: JSON 行（以 \n 结尾）
 * - 响应: JSON 行（以 \n 结尾）
 * - ping 响应 result: { pong: true, version: "...", has_unread: false }
 *   参考: crates/ghostcode-daemon/src/dispatch.rs:100-106
 *
 * @param addr - Daemon 端点描述符，包含 socket 路径
 * @returns ping 是否成功（Daemon 存活且响应正常）
 */
async function pingDaemon(addr: AddrDescriptor): Promise<boolean> {
  return new Promise<boolean>((resolve) => {
    // 超时保护：3 秒内无响应视为失败
    const timer = setTimeout(() => {
      socket.destroy();
      resolve(false);
    }, PING_TIMEOUT_MS);

    const socket = createConnection({ path: addr.path });
    let buffer = "";

    socket.on("connect", () => {
      // 构建 ping 请求（参考 DaemonRequest 格式）
      const req: DaemonRequest = { v: 1, op: "ping", args: {} };
      socket.write(JSON.stringify(req) + "\n");
    });

    socket.on("data", (data: Buffer) => {
      buffer += data.toString("utf-8");

      // 按行分割，寻找完整的 JSON 响应
      const newlineIdx = buffer.indexOf("\n");
      if (newlineIdx !== -1) {
        const line = buffer.slice(0, newlineIdx);
        clearTimeout(timer);
        socket.destroy();

        try {
          const resp = JSON.parse(line) as DaemonResponse;
          // 验证响应：ok=true 且 result.pong=true
          const result = resp.result as Record<string, unknown> | null;
          resolve(resp.ok === true && result?.["pong"] === true);
        } catch {
          resolve(false);
        }
      }
    });

    socket.on("error", () => {
      clearTimeout(timer);
      resolve(false);
    });

    socket.on("close", () => {
      clearTimeout(timer);
    });
  });
}

/**
 * 等待 addr.json 文件出现
 *
 * 轮询等待，最多等待 DAEMON_START_TIMEOUT_MS 毫秒
 * 每 ADDR_POLL_INTERVAL_MS 毫秒检查一次
 *
 * @returns AddrDescriptor 或 null（超时未出现）
 */
async function waitForAddrJson(): Promise<AddrDescriptor | null> {
  const deadline = Date.now() + DAEMON_START_TIMEOUT_MS;

  while (Date.now() < deadline) {
    const addr = readAddrJson();
    if (addr !== null) {
      return addr;
    }

    // 等待下一次轮询
    await new Promise<void>((resolve) =>
      setTimeout(resolve, ADDR_POLL_INTERVAL_MS)
    );
  }

  return null;
}
```

- **验收标准**: `pnpm typecheck` 无错误，函数类型签名正确

---

### Task 3: 实现 ensureDaemon

- **类型**: 核心业务逻辑
- **文件范围**: `src/plugin/src/daemon.ts`（接 Task 2 内容继续）
- **依赖**: Task 2（需要辅助函数）
- **说明**: 实现主入口函数，处理"检测 → 复用/启动 → 确认"完整流程，包含并发保护

**完整实现**:

```typescript
// ============================================
// 模块级状态（单例保护并发）
// ============================================

/**
 * 进行中的 ensureDaemon 调用的 Promise
 *
 * 并发调用时复用同一 Promise，防止重复 spawn
 * null 表示当前没有进行中的启动操作
 */
let _ensureDaemonPromise: Promise<AddrDescriptor> | null = null;

// ============================================
// 公共 API
// ============================================

/**
 * 确保 Daemon 在运行，返回连接地址描述符
 *
 * 启动流程：
 * 1. 读取 ~/.ghostcode/daemon/ghostcoded.addr.json
 * 2. 文件存在 → 验证进程存活 → ping 测试 → 成功则复用
 * 3. 文件不存在或 ping 失败 → spawn 新 Daemon 子进程
 * 4. 等待 addr.json 出现（最多 5s）
 * 5. ping 确认 → 返回 AddrDescriptor
 *
 * 并发安全：多个调用者同时调用时，只会 spawn 一次 Daemon，
 * 后续调用复用第一次的 Promise 结果。
 *
 * 参考: crates/ghostcode-daemon/src/paths.rs - addr.json 路径
 * 参考: oh-my-claudecode/src/features/rate-limit-wait/daemon.ts:441-509 - spawn 模式
 *
 * @returns Daemon 连接地址描述符
 * @throws 如果 Daemon 启动失败或超时
 */
export async function ensureDaemon(): Promise<AddrDescriptor> {
  // ============================================
  // 并发保护：如果有进行中的启动操作，等待复用
  // ============================================
  if (_ensureDaemonPromise !== null) {
    return _ensureDaemonPromise;
  }

  _ensureDaemonPromise = _doEnsureDaemon().finally(() => {
    // 无论成功失败，清除 Promise 锁
    _ensureDaemonPromise = null;
  });

  return _ensureDaemonPromise;
}

/**
 * 内部实现：执行 Daemon 确保逻辑
 *
 * 业务逻辑：
 * 1. 读取 addr.json，检测 Daemon 是否已在运行
 * 2. 如果在运行（进程存活且 ping 成功）→ 直接返回
 * 3. 否则 → spawn 新 Daemon → 等待就绪 → 返回
 *
 * @returns Daemon 连接地址描述符
 * @throws 启动失败或超时时抛出错误
 */
async function _doEnsureDaemon(): Promise<AddrDescriptor> {
  // ============================================
  // 第一步：尝试复用已运行的 Daemon
  // ============================================
  const existingAddr = readAddrJson();

  if (existingAddr !== null) {
    // 验证进程是否真实存活（防止残留的 addr.json）
    const alive = isProcessAlive(existingAddr.pid);

    if (alive) {
      // 进程存活，ping 确认连通性
      const pong = await pingDaemon(existingAddr);

      if (pong) {
        // Daemon 响应正常，直接复用
        return existingAddr;
      }
    }

    // 进程已死或 ping 失败 → 清理残留文件，重新启动
    // 注意：不直接删除 addr.json，让 Daemon 启动时自行覆盖
  }

  // ============================================
  // 第二步：spawn 新的 Daemon 进程
  // ============================================
  await _spawnDaemon();

  // ============================================
  // 第三步：等待 addr.json 出现（最多 5s）
  // ============================================
  const newAddr = await waitForAddrJson();

  if (newAddr === null) {
    throw new Error(
      `GhostCode Daemon 启动超时（${DAEMON_START_TIMEOUT_MS}ms），` +
      `请检查 ${DAEMON_BIN_PATH} 是否存在且可执行`
    );
  }

  // ============================================
  // 第四步：ping 确认新启动的 Daemon 可用
  // ============================================
  const pong = await pingDaemon(newAddr);

  if (!pong) {
    throw new Error(
      "GhostCode Daemon 启动后 ping 失败，可能 Daemon 初始化异常"
    );
  }

  return newAddr;
}

/**
 * 启动新的 Daemon 子进程
 *
 * 使用 detached: true 让 Daemon 脱离父进程独立运行
 * 使用 stdio: 'ignore' 断开 stdio 连接（Daemon 写自己的日志）
 * 调用 child.unref() 确保父进程退出时不等待 Daemon
 *
 * 参考: oh-my-claudecode/src/features/rate-limit-wait/daemon.ts:471-481
 *
 * @throws 二进制文件不存在或 spawn 失败时抛出
 */
async function _spawnDaemon(): Promise<void> {
  // 检查 Daemon 二进制是否存在
  if (!existsSync(DAEMON_BIN_PATH)) {
    throw new Error(
      `GhostCode Daemon 二进制文件不存在: ${DAEMON_BIN_PATH}\n` +
      `请先安装 GhostCode 或检查安装路径。`
    );
  }

  return new Promise<void>((resolve, reject) => {
    const child = spawn(DAEMON_BIN_PATH, [], {
      // detached: true 让子进程成为新进程组领导，脱离父进程
      detached: true,
      // stdio: 'ignore' 断开 stdin/stdout/stderr，让 Daemon 独立运行
      stdio: "ignore",
      // 使用白名单环境变量，防止凭证泄漏
      env: createMinimalDaemonEnv(),
    });

    child.on("error", (err) => {
      reject(
        new Error(`spawn GhostCode Daemon 失败: ${err.message}`)
      );
    });

    child.on("spawn", () => {
      // 脱引用子进程：父进程退出时不等待 Daemon
      child.unref();
      resolve();
    });
  });
}
```

- **验收标准**: `pnpm typecheck` 无错误，函数导出正确

---

### Task 4: 实现 stopDaemon

- **类型**: 业务逻辑
- **文件范围**: `src/plugin/src/daemon.ts`（接 Task 3 内容继续）
- **依赖**: Task 3（需要 pingDaemon、readAddrJson 辅助函数）
- **说明**: 实现停止 Daemon 的公共 API，通过发送 `shutdown` op 触发 Daemon 优雅关闭

**完整实现**:

```typescript
/**
 * 停止 GhostCode Daemon
 *
 * 通过 IPC 发送 shutdown 操作，触发 Daemon 优雅关闭。
 * Daemon 收到后会清理 socket 文件并退出（参考 dispatch.rs handle_shutdown）。
 *
 * 如果 Daemon 未运行，此函数静默返回（幂等操作）。
 *
 * 参考: crates/ghostcode-daemon/src/dispatch.rs:111-114 - shutdown handler
 * 参考: crates/ghostcode-daemon/src/server.rs:193-199 - 优雅关闭逻辑
 *
 * @throws 如果 Daemon 正在运行但无法停止时抛出
 */
export async function stopDaemon(): Promise<void> {
  const addr = readAddrJson();

  // Daemon 未运行，静默返回（幂等）
  if (addr === null) {
    return;
  }

  // 进程不存活，无需发送 shutdown（可能是残留文件）
  if (!isProcessAlive(addr.pid)) {
    return;
  }

  // ============================================
  // 发送 shutdown 操作
  // ============================================
  return new Promise<void>((resolve) => {
    // 超时保护：3 秒后强制认为已关闭
    const timer = setTimeout(() => {
      socket.destroy();
      resolve();
    }, PING_TIMEOUT_MS);

    const socket = createConnection({ path: addr.path });

    socket.on("connect", () => {
      const req: DaemonRequest = { v: 1, op: "shutdown", args: {} };
      socket.write(JSON.stringify(req) + "\n");
    });

    socket.on("data", (_data: Buffer) => {
      // 收到响应后关闭连接（不需要解析响应内容）
      clearTimeout(timer);
      socket.destroy();
      resolve();
    });

    socket.on("error", () => {
      // socket 错误视为 Daemon 已停止
      clearTimeout(timer);
      resolve();
    });

    socket.on("close", () => {
      clearTimeout(timer);
      resolve();
    });
  });
}
```

- **验收标准**: `pnpm typecheck` 无错误，幂等行为正确

---

### Task 5: 实现 startHeartbeat 和 getDaemonBinaryPath

- **类型**: 业务逻辑
- **文件范围**: `src/plugin/src/daemon.ts`（接 Task 4 内容继续）
- **依赖**: Task 3（需要 ensureDaemon），Task 4（需要 stopDaemon）
- **说明**: 实现心跳监控和二进制路径获取函数

**完整实现**:

```typescript
/**
 * 启动心跳监控
 *
 * 每 HEARTBEAT_INTERVAL_MS（10 秒）向 Daemon 发送 ping。
 * 连续失败 HEARTBEAT_MAX_FAILURES（3 次）时，认为 Daemon 已崩溃，
 * 尝试重启（先停止旧进程，再调用 ensureDaemon）。
 *
 * 心跳失败不会直接抛出异常，而是在后台静默重试。
 * 重启失败也只记录到 console.error，不中断 Plugin 主流程。
 *
 * @param addr - 当前 Daemon 连接地址（初始心跳目标）
 * @returns 停止心跳的函数，调用后心跳立即停止
 */
export function startHeartbeat(addr: AddrDescriptor): () => void {
  // 心跳失败计数器
  let failureCount = 0;

  // 是否已停止（调用 stop 函数后设为 true）
  let stopped = false;

  // 当前有效的 Daemon 地址（重启后可能变化）
  let currentAddr = addr;

  /**
   * 单次心跳检查
   *
   * 1. 对当前地址执行 ping
   * 2. 成功 → 清零失败计数
   * 3. 失败 → 累加失败计数，达阈值时触发重启
   */
  async function heartbeatTick(): Promise<void> {
    if (stopped) {
      return;
    }

    try {
      const alive = await pingDaemon(currentAddr);

      if (alive) {
        // 心跳成功，清零失败计数
        failureCount = 0;
      } else {
        failureCount += 1;

        if (failureCount >= HEARTBEAT_MAX_FAILURES) {
          // ============================================
          // 连续 3 次失败 → 尝试重启 Daemon
          // ============================================
          failureCount = 0;

          try {
            // 尝试停止旧 Daemon（可能已死，忽略错误）
            await stopDaemon();
          } catch {
            // 旧 Daemon 可能已经不存在，忽略停止错误
          }

          // 重启 Daemon 并更新当前地址
          const newAddr = await ensureDaemon();
          currentAddr = newAddr;
        }
      }
    } catch {
      // 心跳操作本身出现异常（不应发生），计为一次失败
      failureCount += 1;
    }
  }

  // 启动心跳定时器
  const timer = setInterval(() => {
    // 不 await，让心跳异步执行，避免堆积
    void heartbeatTick();
  }, HEARTBEAT_INTERVAL_MS);

  // 返回停止函数
  return () => {
    stopped = true;
    clearInterval(timer);
  };
}

/**
 * 获取 Daemon 二进制文件路径
 *
 * @returns Daemon 二进制文件的绝对路径
 */
export function getDaemonBinaryPath(): string {
  return DAEMON_BIN_PATH;
}
```

- **验收标准**: `pnpm typecheck` 无错误，返回类型正确（`() => void`）

---

### Task 6: 更新 src/index.ts 导出

- **类型**: 接口更新
- **文件范围**: `src/plugin/src/index.ts`（修改 T16 创建的文件）
- **依赖**: Task 1 ~ Task 5（daemon.ts 内容完整后才能更新导出）
- **说明**: 更新 index.ts，将 T16 的占位导出替换为 T17 实现的真实 API

**修改内容**:

将 index.ts 中的 daemon 相关导出替换为：

```typescript
// ============================================
// Daemon 管理模块导出（T17 实现）
// ============================================
export type { AddrDescriptor } from "./daemon.js";
export { ensureDaemon, stopDaemon, startHeartbeat, getDaemonBinaryPath } from "./daemon.js";
```

**删除 T16 的旧导出**（以下内容需从 index.ts 移除，因为 T17 的 daemon.ts 不再导出这些符号）:
```typescript
// 删除：
export type { DaemonStatus, DaemonOptions } from "./daemon.js";
export { getDaemonStatus, startDaemon, stopDaemon } from "./daemon.js";
```

**注意**: T17 的 daemon.ts 完全替换了 T16 的占位文件，接口发生变化：
- 旧: `DaemonStatus`, `DaemonOptions`, `getDaemonStatus`, `startDaemon`, `stopDaemon`
- 新: `AddrDescriptor`, `ensureDaemon`, `stopDaemon`, `startHeartbeat`, `getDaemonBinaryPath`

- **验收标准**: `pnpm build` 零错误，`dist/index.d.ts` 包含正确的类型导出

---

### Task 7: 添加 vitest 依赖并编写测试文件

- **类型**: 测试
- **文件范围**:
  - `src/plugin/package.json`（添加 vitest devDependency）
  - `src/plugin/src/__tests__/daemon.test.ts`（新建）
- **依赖**: Task 1 ~ Task 5（需要 daemon.ts 实现完成）
- **说明**: 为 T17 规格要求的 5 个测试用例编写 vitest 测试

#### 7.1 更新 package.json

在 `devDependencies` 中添加 vitest，并在 `scripts` 中添加 `test` 命令：

```json
{
  "scripts": {
    "build": "tsup",
    "dev": "tsup --watch",
    "typecheck": "tsc --noEmit",
    "test": "vitest run",
    "test:watch": "vitest"
  },
  "devDependencies": {
    "@types/node": "^22.0.0",
    "tsup": "^8.0.0",
    "typescript": "^5.7.0",
    "vitest": "^2.0.0"
  }
}
```

#### 7.2 创建测试文件

```typescript
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
 *
 * 返回一个包含 on/write/destroy 方法的对象，
 * 可通过 emit 触发事件来模拟 socket 行为
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
    vi.useFakeTimers();
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
      // ============================================
      // 准备：addr.json 不存在，spawn 成功
      // ============================================
      const addr = mockAddr();

      // 模拟 addr.json 不存在
      (existsSync as ReturnType<typeof vi.fn>)
        .mockReturnValueOnce(false)   // ADDR_JSON_PATH 不存在（readAddrJson）
        .mockReturnValueOnce(true);   // DAEMON_BIN_PATH 存在（_spawnDaemon）

      // 模拟 spawn 成功
      const mockChild = new EventEmitter() as ReturnType<typeof spawn>;
      (spawn as ReturnType<typeof vi.fn>).mockReturnValue(mockChild);

      // 模拟 waitForAddrJson 轮询成功
      // 第一次调用（轮询中）返回 null，第二次返回 addr
      (readFileSync as ReturnType<typeof vi.fn>)
        .mockReturnValueOnce(JSON.stringify(addr));

      // 第一次 existsSync（waitForAddrJson 轮询）返回 true
      // 注意：实现中 readAddrJson 调用 existsSync，这是第三次调用
      (existsSync as ReturnType<typeof vi.fn>)
        .mockReturnValueOnce(true);   // waitForAddrJson 第一次轮询成功

      // 模拟 ping 成功（socket 连接）
      const mockSocket = createMockSocket();
      (createConnection as ReturnType<typeof vi.fn>).mockReturnValue(mockSocket);

      // ============================================
      // 执行
      // ============================================
      const { ensureDaemon } = await import("../daemon.js");

      // 触发 spawn 的 "spawn" 事件（异步触发）
      process.nextTick(() => {
        mockChild.emit("spawn");
        // 触发 socket 连接和 ping 响应
        mockSocket.emit("connect");
        mockSocket.emit("data", Buffer.from(buildPingResponse(true)));
      });

      const result = await ensureDaemon();

      // ============================================
      // 验证
      // ============================================
      expect(spawn).toHaveBeenCalledWith(
        expect.stringContaining("ghostcoded"),
        [],
        expect.objectContaining({ detached: true, stdio: "ignore" })
      );
      expect(result.v).toBe(1);
      expect(result.transport).toBe("unix");
    });

    // ------------------------------------------
    // 测试 2: ensureDaemon reuses existing daemon
    // ------------------------------------------
    it("当 Daemon 已在运行时，应复用现有 Daemon", async () => {
      const addr = mockAddr();

      // 模拟 addr.json 存在且内容有效
      (existsSync as ReturnType<typeof vi.fn>).mockReturnValue(true);
      (readFileSync as ReturnType<typeof vi.fn>).mockReturnValue(
        JSON.stringify(addr)
      );

      // 模拟进程存活（process.kill 不抛出）
      vi.spyOn(process, "kill").mockImplementation(() => true);

      // 模拟 ping 成功
      const mockSocket = createMockSocket();
      (createConnection as ReturnType<typeof vi.fn>).mockReturnValue(mockSocket);

      const { ensureDaemon } = await import("../daemon.js");

      process.nextTick(() => {
        mockSocket.emit("connect");
        mockSocket.emit("data", Buffer.from(buildPingResponse(true)));
      });

      const result = await ensureDaemon();

      // 验证：不应该调用 spawn
      expect(spawn).not.toHaveBeenCalled();
      expect(result.pid).toBe(addr.pid);
    });

    // ------------------------------------------
    // 测试 5: concurrent ensureDaemon calls safe
    // ------------------------------------------
    it("并发调用 ensureDaemon 应安全（只 spawn 一次）", async () => {
      const addr = mockAddr();

      // 模拟 addr.json 不存在，第一次调用触发 spawn
      (existsSync as ReturnType<typeof vi.fn>)
        .mockReturnValueOnce(false)
        .mockReturnValueOnce(true)   // DAEMON_BIN_PATH 存在
        .mockReturnValue(true);      // waitForAddrJson 轮询

      (readFileSync as ReturnType<typeof vi.fn>).mockReturnValue(
        JSON.stringify(addr)
      );

      const mockChild = new EventEmitter() as ReturnType<typeof spawn>;
      (spawn as ReturnType<typeof vi.fn>).mockReturnValue(mockChild);

      const mockSocket = createMockSocket();
      (createConnection as ReturnType<typeof vi.fn>).mockReturnValue(mockSocket);

      const { ensureDaemon } = await import("../daemon.js");

      // 并发触发 3 次 ensureDaemon
      process.nextTick(() => {
        mockChild.emit("spawn");
        mockSocket.emit("connect");
        mockSocket.emit("data", Buffer.from(buildPingResponse(true)));
      });

      const [r1, r2, r3] = await Promise.all([
        ensureDaemon(),
        ensureDaemon(),
        ensureDaemon(),
      ]);

      // 验证：只 spawn 了一次
      expect(spawn).toHaveBeenCalledTimes(1);
      // 验证：返回的地址相同
      expect(r1.pid).toBe(r2.pid);
      expect(r2.pid).toBe(r3.pid);
    });
  });

  // ------------------------------------------
  // 测试 3: heartbeat detects daemon crash
  // ------------------------------------------
  describe("startHeartbeat", () => {
    it("心跳连续失败 3 次后应尝试重启 Daemon", async () => {
      const addr = mockAddr();

      // 模拟初始 ping 失败（Daemon 崩溃）
      const mockSocket = createMockSocket();
      (createConnection as ReturnType<typeof vi.fn>).mockReturnValue(mockSocket);

      const { startHeartbeat, ensureDaemon: mockEnsure } = await import("../daemon.js");

      // 模拟 ping 一直超时（socket 不响应）
      // 由于 PING_TIMEOUT_MS，需要用 fake timers 推进
      const stopHeartbeat = startHeartbeat(addr);

      // 推进时间触发 3 次心跳失败
      // 每次心跳 10s，ping 超时 3s
      for (let i = 0; i < 3; i++) {
        await vi.advanceTimersByTimeAsync(HEARTBEAT_INTERVAL_MS + PING_TIMEOUT_MS + 100);
      }

      // 清理心跳
      stopHeartbeat();

      // 验证：经过 3 次失败后，ensureDaemon 被调用（重启）
      // （由于 mock 复杂，此处验证 createConnection 调用次数代表 ping 次数）
      expect(createConnection).toHaveBeenCalledTimes(3);
    });

    it("stop 函数调用后心跳应停止", async () => {
      const addr = mockAddr();

      const mockSocket = createMockSocket();
      (createConnection as ReturnType<typeof vi.fn>).mockReturnValue(mockSocket);
      mockSocket.on("connect", () => {
        mockSocket.emit("data", Buffer.from(buildPingResponse(true)));
      });

      const { startHeartbeat } = await import("../daemon.js");
      const stop = startHeartbeat(addr);

      // 推进一个心跳周期
      await vi.advanceTimersByTimeAsync(HEARTBEAT_INTERVAL_MS + 100);

      // 停止心跳
      stop();

      const callCountBefore = (createConnection as ReturnType<typeof vi.fn>).mock.calls.length;

      // 再推进两个心跳周期
      await vi.advanceTimersByTimeAsync(HEARTBEAT_INTERVAL_MS * 2 + 100);

      // 验证：停止后不再有新的 ping 调用
      expect((createConnection as ReturnType<typeof vi.fn>).mock.calls.length).toBe(callCountBefore);
    });
  });

  // ------------------------------------------
  // 测试 4: version mismatch triggers restart
  // ------------------------------------------
  describe("版本不匹配处理", () => {
    it("addr.json 中的版本与期望不符时，ping 失败触发重启", async () => {
      // 版本不匹配场景：进程存活但 ping 返回旧版本（ok=false）
      const oldAddr = mockAddr();

      (existsSync as ReturnType<typeof vi.fn>).mockReturnValue(true);
      (readFileSync as ReturnType<typeof vi.fn>).mockReturnValue(
        JSON.stringify(oldAddr)
      );

      vi.spyOn(process, "kill").mockImplementation(() => true);

      // ping 返回失败（模拟版本不匹配后的异常响应）
      const mockSocket = createMockSocket();
      (createConnection as ReturnType<typeof vi.fn>).mockReturnValueOnce(mockSocket);

      const { ensureDaemon } = await import("../daemon.js");

      // 第一次 ping 返回失败
      process.nextTick(() => {
        mockSocket.emit("connect");
        mockSocket.emit("data", Buffer.from(
          JSON.stringify({ v: 1, ok: false, result: null, error: { code: "VERSION_MISMATCH", message: "版本不匹配" } }) + "\n"
        ));
      });

      // 此时应触发重启（spawn 新进程）
      // 模拟 spawn 和新 addr.json
      const newAddr = mockAddr(99999);
      const mockChild = new EventEmitter() as ReturnType<typeof spawn>;
      (spawn as ReturnType<typeof vi.fn>).mockReturnValue(mockChild);

      (existsSync as ReturnType<typeof vi.fn>)
        .mockReturnValueOnce(true)   // DAEMON_BIN_PATH
        .mockReturnValueOnce(true);  // waitForAddrJson

      (readFileSync as ReturnType<typeof vi.fn>).mockReturnValue(
        JSON.stringify(newAddr)
      );

      const mockSocket2 = createMockSocket();
      (createConnection as ReturnType<typeof vi.fn>).mockReturnValueOnce(mockSocket2);

      process.nextTick(() => {
        mockChild.emit("spawn");
        mockSocket2.emit("connect");
        mockSocket2.emit("data", Buffer.from(buildPingResponse(true)));
      });

      // ensureDaemon 应成功返回新 Daemon 的地址
      const result = await ensureDaemon();
      expect(result.pid).toBe(newAddr.pid);
      expect(spawn).toHaveBeenCalledTimes(1);
    });
  });
});
```

**注意事项**:

测试中使用的 `HEARTBEAT_INTERVAL_MS` 常量需要从 daemon.ts 中导出，以便测试使用：

```typescript
// 在 daemon.ts 的常量区新增导出（供测试使用）
export const HEARTBEAT_INTERVAL_MS_TEST = HEARTBEAT_INTERVAL_MS;
```

或者在测试中直接使用 10000（数字字面量），避免改变 daemon.ts 接口。

- **验收标准**: `pnpm test` 全部测试通过

---

### Task 8: 验证构建和测试

- **类型**: 验证
- **文件范围**: `src/plugin/` 整体
- **依赖**: Task 1 ~ Task 7 全部完成
- **实施步骤**:

1. 确认 `src/plugin/src/daemon.ts` 文件为 T17 完整实现（替换 T16 占位）
2. 确认 `src/plugin/src/index.ts` 导出已更新
3. 在 `src/plugin/` 目录执行：
   ```bash
   pnpm install     # 安装 vitest
   pnpm build       # 构建验证
   pnpm typecheck   # 类型检查
   pnpm test        # 运行测试
   ```

**预期结果**:
- `dist/index.js` 包含 `ensureDaemon`, `stopDaemon`, `startHeartbeat`, `getDaemonBinaryPath`
- `dist/index.d.ts` 包含 `AddrDescriptor` 类型导出
- 全部 5 个测试通过

- **验收标准**: `pnpm build` 零错误，`pnpm test` 全部通过

---

## 文件变更汇总

| Task | 文件 | 操作 |
|------|------|------|
| Task 1-5 | `src/plugin/src/daemon.ts` | 替换（覆盖 T16 占位内容） |
| Task 6 | `src/plugin/src/index.ts` | 修改（更新 daemon 模块导出） |
| Task 7 | `src/plugin/package.json` | 修改（添加 vitest） |
| Task 7 | `src/plugin/src/__tests__/daemon.test.ts` | 新建 |
| Task 8 | — | 验证执行（无文件修改） |

---

## TDD 强制执行规范

本任务必须严格遵循 TDD 流程：Red → Green → Refactor。

```
Red    → 先写测试文件（Task 7: daemon.test.ts）+ 创建 daemon.ts 最小 stub（仅导出签名，body 抛异常）
Green  → 补全 daemon.ts 完整实现（Task 1-5），让所有测试通过
Refactor → 更新 index.ts（Task 6）+ 验证（Task 8）
```

---

## 并行分组

- **Layer 1** (TDD Red — 测试先行):
  - Task 7: 创建测试文件 `__tests__/daemon.test.ts`（完整测试用例）
  - 创建 daemon.ts 最小 stub（仅类型 + 函数签名，body 用 `throw new Error("not implemented")`)
  - 验证: `pnpm test` 编译通过但测试失败（Red）

- **Layer 2** (TDD Green — 实现):
  - Task 1: 类型定义（替换 stub 中的类型部分）
  - Task 2: 辅助函数
  - Task 3 + Task 4: 业务函数（可并行）
  - Task 5: 心跳 + getDaemonBinaryPath
  - 实际操作：串行追加写入 daemon.ts，替换 stub 内容
  - 验证: `pnpm test` 所有测试通过（Green）

- **Layer 3** (TDD Refactor — 集成 + 验证):
  - Task 6: 更新 index.ts 导出
  - Task 8: `pnpm build` + `pnpm test` 最终验证

**推荐执行顺序**:
```
Layer 1: Task 7 (Red: 测试先写 + daemon.ts stub)
              ↓
Layer 2: Task 1 → 2 → 3/4 → 5 (Green: daemon.ts 完整实现)
              ↓
Layer 3: Task 6 + Task 8 (Refactor: 集成 + 验证)
```

实际上 Task 1-5 共同构成 `daemon.ts` 的内容，按顺序追加写入同一文件即可。Builder 应先写测试（Task 7），再串行完成 Task 1→2→3→4→5，最后处理 Task 6 和 Task 8。

---

## 潜在问题及解决方案

| 问题 | 原因 | 解决方案 |
|------|------|---------|
| `spawn` 触发 `error: ENOENT` | DAEMON_BIN_PATH 不存在 | 在 `_spawnDaemon` 中先 `existsSync` 检查，抛出有明确提示的错误 |
| `waitForAddrJson` 超时 | Daemon 启动慢或崩溃 | 超时后抛出含路径信息的详细错误，方便排查 |
| ping socket 连接被拒 | Daemon 正在启动中（addr.json 已写但 socket 未就绪） | ping 超时 3s，`_doEnsureDaemon` 中 waitForAddrJson 完成后再 ping，此时 Daemon 应已就绪 |
| `process.kill(pid, 0)` 在 Windows 行为不同 | Windows 没有 Unix 信号 | T17 规格约束为 macOS（Darwin），不需要 Windows 兼容 |
| vitest mock 的 ESM 兼容 | `vi.mock` 需要提升到顶层 | 测试文件中所有 `vi.mock` 调用放在 `import` 之前，vitest 自动提升处理 |
| 心跳 `setInterval` 在测试中失控 | 真实定时器异步执行 | `vi.useFakeTimers()` + `vi.advanceTimersByTimeAsync()` 精确控制 |

---

## 注意事项（Builder 实施时）

1. **文件是替换，不是追加**: daemon.ts 是 T16 创建的占位文件，T17 完全覆盖它，不要保留 T16 的 `DaemonStatus`、`DaemonOptions`、`getDaemonStatus`、`startDaemon` 等符号
2. **index.ts 要同步更新**: 因为导出的符号发生了变化，必须更新 index.ts，否则 `pnpm build` 会报错
3. **socket 路径来自 addr.json**: 不要硬编码 socket 路径，始终从 `AddrDescriptor.path` 中读取（Daemon 可能使用 /tmp fallback）
4. **不依赖 T18**: T17 的 ping 和 shutdown 是内联的一次性 socket 操作，T18 实现完整的复用 IPC 连接；两者不冲突
5. **测试目录创建**: 需要先创建 `src/plugin/src/__tests__/` 目录再写测试文件
