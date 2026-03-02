/**
 * @file Daemon 生命周期管理
 * @description 管理 GhostCode Rust Daemon 进程的启动、停止、健康检查和心跳监控。
 *              负责读取 ~/.ghostcode/daemon/ghostcoded.addr.json 获取 Daemon 连接信息，
 *              在 Daemon 未运行时自动 spawn 新进程，并通过心跳维持连接存活。
 *
 *              核心流程：
 *              1. 读取 addr.json -> 检测 Daemon 是否在运行
 *              2. 不存在或 ping 失败 -> spawn ghostcoded 二进制
 *              3. 等待 addr.json 出现（最多 5s）-> ping 确认
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
// 类型定义
// 复用 ipc.ts 中的协议类型，避免重复定义
// AddrDescriptor 是 daemon.ts 独有的（addr.json 描述符）
// ============================================

import type { DaemonRequest, DaemonResponse } from "./ipc.js";

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
 * @param addr - Daemon 端点描述符，包含 socket 路径
 * @returns ping 是否成功（Daemon 存活且响应正常）
 */
async function pingDaemon(addr: AddrDescriptor): Promise<boolean> {
  return new Promise<boolean>((resolve) => {
    const timer = setTimeout(() => {
      socket.destroy();
      resolve(false);
    }, PING_TIMEOUT_MS);

    const socket = createConnection({ path: addr.path });
    let buffer = "";

    socket.on("connect", () => {
      const req: DaemonRequest = { v: 1, op: "ping", args: {} };
      socket.write(JSON.stringify(req) + "\n");
    });

    socket.on("data", (data: Buffer) => {
      buffer += data.toString("utf-8");

      const newlineIdx = buffer.indexOf("\n");
      if (newlineIdx !== -1) {
        const line = buffer.slice(0, newlineIdx);
        clearTimeout(timer);
        socket.destroy();

        try {
          const resp = JSON.parse(line) as DaemonResponse;
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
      // 连接关闭且无响应/错误时，视为 ping 失败（防止 Promise 永久 pending）
      resolve(false);
    });
  });
}

/**
 * 等待 addr.json 文件出现
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

    await new Promise<void>((resolve) =>
      setTimeout(resolve, ADDR_POLL_INTERVAL_MS)
    );
  }

  return null;
}

// ============================================
// 模块级状态（单例保护并发 + 成功缓存）
// ============================================

/** 启动中的 Promise（并发保护：多个调用者共享同一个启动过程） */
let _startingPromise: Promise<AddrDescriptor> | null = null;

/** 已成功启动的 addr 缓存（避免重复 ping/spawn） */
let _cachedAddr: AddrDescriptor | null = null;

// ============================================
// 公共 API
// ============================================

/**
 * 确保 Daemon 在运行，返回连接地址描述符
 *
 * 并发安全：多个调用者同时调用时，只会 spawn 一次 Daemon。
 * 成功缓存：Daemon 启动成功后缓存 addr，后续调用直接复用（检查进程存活）。
 *
 * @returns Daemon 连接地址描述符
 * @throws 如果 Daemon 启动失败或超时
 */
export async function ensureDaemon(): Promise<AddrDescriptor> {
  // 快速路径：已有缓存的 addr，检查进程是否存活
  if (_cachedAddr !== null) {
    if (isProcessAlive(_cachedAddr.pid)) {
      return _cachedAddr;
    }
    // 进程已死，清空缓存
    _cachedAddr = null;
  }

  // 正在启动中：等待已有的 Promise
  if (_startingPromise !== null) {
    return _startingPromise;
  }

  // 发起新的启动流程
  _startingPromise = _doEnsureDaemon()
    .then((addr) => {
      _cachedAddr = addr; // 成功时缓存
      return addr;
    })
    .finally(() => {
      _startingPromise = null; // 无论成败，清空"正在启动"标志
    });

  return _startingPromise;
}

async function _doEnsureDaemon(): Promise<AddrDescriptor> {
  // 第一步：尝试复用已运行的 Daemon
  const existingAddr = readAddrJson();

  if (existingAddr !== null) {
    const alive = isProcessAlive(existingAddr.pid);

    if (alive) {
      const pong = await pingDaemon(existingAddr);

      if (pong) {
        return existingAddr;
      }
    }
  }

  // 第二步：spawn 新的 Daemon 进程
  await _spawnDaemon();

  // 第三步：等待 addr.json 出现
  const newAddr = await waitForAddrJson();

  if (newAddr === null) {
    throw new Error(
      `GhostCode Daemon 启动超时（${DAEMON_START_TIMEOUT_MS}ms），` +
      `请检查 ${DAEMON_BIN_PATH} 是否存在且可执行`
    );
  }

  // 第四步：ping 确认新启动的 Daemon 可用
  const pong = await pingDaemon(newAddr);

  if (!pong) {
    throw new Error(
      "GhostCode Daemon 启动后 ping 失败，可能 Daemon 初始化异常"
    );
  }

  return newAddr;
}

async function _spawnDaemon(): Promise<void> {
  if (!existsSync(DAEMON_BIN_PATH)) {
    throw new Error(
      `GhostCode Daemon 二进制文件不存在: ${DAEMON_BIN_PATH}\n` +
      `请先安装 GhostCode 或检查安装路径。`
    );
  }

  return new Promise<void>((resolve, reject) => {
    const child = spawn(DAEMON_BIN_PATH, [], {
      detached: true,
      stdio: "ignore",
      env: createMinimalDaemonEnv(),
    });

    child.on("error", (err) => {
      reject(
        new Error(`spawn GhostCode Daemon 失败: ${err.message}`)
      );
    });

    child.on("spawn", () => {
      child.unref();
      resolve();
    });
  });
}

/**
 * 停止 GhostCode Daemon
 *
 * 通过 IPC 发送 shutdown 操作。如果 Daemon 未运行，静默返回（幂等）。
 */
export async function stopDaemon(): Promise<void> {
  // 清空缓存，确保下次 ensureDaemon 重新检测
  _cachedAddr = null;
  const addr = readAddrJson();

  if (addr === null) {
    return;
  }

  if (!isProcessAlive(addr.pid)) {
    return;
  }

  return new Promise<void>((resolve) => {
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
      clearTimeout(timer);
      socket.destroy();
      resolve();
    });

    socket.on("error", () => {
      clearTimeout(timer);
      resolve();
    });

    socket.on("close", () => {
      clearTimeout(timer);
      resolve();
    });
  });
}

/**
 * 启动心跳监控
 *
 * 每 10s 向 Daemon 发送 ping，连续失败 3 次触发重启。
 *
 * @param addr - 当前 Daemon 连接地址
 * @returns 停止心跳的函数
 */
export function startHeartbeat(addr: AddrDescriptor): () => void {
  let failureCount = 0;
  let stopped = false;
  let currentAddr = addr;

  async function heartbeatTick(): Promise<void> {
    if (stopped) {
      return;
    }

    try {
      const alive = await pingDaemon(currentAddr);

      if (alive) {
        failureCount = 0;
      } else {
        failureCount += 1;

        if (failureCount >= HEARTBEAT_MAX_FAILURES) {
          failureCount = 0;

          try {
            await stopDaemon();
          } catch {
            // 忽略停止错误
          }

          const newAddr = await ensureDaemon();
          currentAddr = newAddr;
        }
      }
    } catch {
      failureCount += 1;
    }
  }

  const timer = setInterval(() => {
    void heartbeatTick();
  }, HEARTBEAT_INTERVAL_MS);

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
