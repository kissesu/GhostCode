/**
 * @file web.ts
 * @description GhostCode Web Dashboard 生命周期管理
 *              管理 ghostcode-web HTTP 服务器的单实例启动、健康检查和浏览器自动打开。
 *              借鉴 claude-mem 的 Worker 单实例模式：
 *              - 多 session 共享一个 Web Server（不重复启动）
 *              - 仅在 Web Server 首次启动时自动打开浏览器
 *              - Web Server 生命周期与 session 解耦（session 结束不关闭 Web Server）
 *
 *              核心流程：
 *              1. 检查 Web Server 是否已运行（HTTP 健康检查）
 *              2. 未运行则 spawn ghostcode-web 二进制
 *              3. 等待健康检查通过
 *              4. 仅首次启动时打开浏览器（已运行时跳过）
 *
 *              参考: claude-mem worker-service.cjs — Worker 单实例管理
 *              参考: daemon.ts — ensureDaemon() 模式（PID + ping + spawn）
 * @author Atlas.oi
 * @date 2026-03-15
 */

import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";
import { openURL } from "./utils/browser.js";

// ============================================
// 常量
// ============================================

/** GhostCode 根目录 */
const GHOSTCODE_HOME = join(homedir(), ".ghostcode");

/** ghostcode-web 二进制路径 */
const WEB_BIN_PATH = join(GHOSTCODE_HOME, "bin", "ghostcode-web");

/** Web Server 默认绑定地址 */
const WEB_HOST = "127.0.0.1";

/** Web Server 默认端口（与 ghostcode-web main.rs 的 --bind 默认值一致） */
const WEB_PORT = 7070;

/** 健康检查超时时间（毫秒） */
const HEALTH_CHECK_TIMEOUT_MS = 2000;

/** 等待 Web Server 启动的最大时间（毫秒） */
const WEB_START_TIMEOUT_MS = 8000;

/** 等待 Web Server 启动的轮询间隔（毫秒） */
const WEB_POLL_INTERVAL_MS = 300;

/**
 * 允许传递给 Web Server 子进程的环境变量白名单
 *
 * 遵循最小权限原则，防止凭证泄漏
 * 参考: daemon.ts:97-106 — DAEMON_ENV_ALLOWLIST
 */
const WEB_ENV_ALLOWLIST = [
  "PATH", "HOME",
  "USER", "USERNAME", "LOGNAME",
  "LANG", "LC_ALL", "LC_CTYPE",
  "TERM",
  "TMPDIR", "TMP", "TEMP",
  "XDG_RUNTIME_DIR", "XDG_DATA_HOME", "XDG_CONFIG_HOME",
  "SHELL",
  "NODE_ENV",
  "RUST_LOG",
] as const;

// ============================================
// 模块级状态（单例保护）
// ============================================

/** 启动中的 Promise（并发保护：多个调用者共享同一个启动过程） */
let _startingPromise: Promise<void> | null = null;

/** 是否已完成 ensureWeb（缓存结果，避免重复检查） */
let _webReady = false;

// ============================================
// 辅助函数
// ============================================

/**
 * 创建最小化的 Web Server 进程环境变量
 *
 * @returns 过滤后的环境变量对象
 */
function createMinimalWebEnv(): NodeJS.ProcessEnv {
  const env: NodeJS.ProcessEnv = {};
  for (const key of WEB_ENV_ALLOWLIST) {
    if (process.env[key] !== undefined) {
      env[key] = process.env[key];
    }
  }
  return env;
}

/**
 * 通过 HTTP 健康检查判断 Web Server 是否在运行
 *
 * 业务逻辑：
 * 1. 请求 http://127.0.0.1:7070/health
 * 2. 返回 200 则认为 Web Server 正在运行
 * 3. 连接拒绝或超时则认为未运行
 *
 * @returns Web Server 是否在运行
 */
async function isWebRunning(): Promise<boolean> {
  try {
    const response = await fetch(`http://${WEB_HOST}:${WEB_PORT}/health`, {
      signal: AbortSignal.timeout(HEALTH_CHECK_TIMEOUT_MS),
    });
    return response.ok;
  } catch {
    return false;
  }
}

/**
 * 等待 Web Server 健康检查通过
 *
 * @returns 是否在超时前通过健康检查
 */
async function waitForWebReady(): Promise<boolean> {
  const deadline = Date.now() + WEB_START_TIMEOUT_MS;

  while (Date.now() < deadline) {
    if (await isWebRunning()) {
      return true;
    }
    await new Promise<void>((resolve) =>
      setTimeout(resolve, WEB_POLL_INTERVAL_MS)
    );
  }

  return false;
}

/**
 * Spawn ghostcode-web 后台进程
 *
 * 以 detached 模式启动，与当前 Node.js 进程生命周期解耦。
 * ghostcode-web 内部有 kill_stale_listener 机制，不会重复绑定端口。
 *
 * @throws ghostcode-web 二进制不存在时抛出错误
 */
async function spawnWeb(): Promise<void> {
  if (!existsSync(WEB_BIN_PATH)) {
    throw new Error(
      `ghostcode-web 二进制文件不存在: ${WEB_BIN_PATH}\n` +
      `请先编译: cargo build --release -p ghostcode-web`
    );
  }

  return new Promise<void>((resolve, reject) => {
    const child = spawn(WEB_BIN_PATH, [], {
      detached: true,
      stdio: "ignore",
      env: createMinimalWebEnv(),
    });

    child.on("error", (err) => {
      reject(new Error(`spawn ghostcode-web 失败: ${err.message}`));
    });

    child.on("spawn", () => {
      child.unref();
      resolve();
    });
  });
}

// ============================================
// 公共 API
// ============================================

/**
 * 确保 Web Dashboard 在运行，必要时自动打开浏览器
 *
 * 单实例保证机制（借鉴 claude-mem 模式）：
 * 1. HTTP 健康检查判断 Web Server 是否已运行
 * 2. 已运行 → 直接返回（不打开浏览器，复用已有实例）
 * 3. 未运行 → spawn 新进程 → 等待就绪 → 打开浏览器
 *
 * 并发安全：多个调用者同时调用时，只会启动一次 Web Server。
 *
 * @returns Dashboard URL
 */
export async function ensureWeb(): Promise<string> {
  const dashboardUrl = `http://${WEB_HOST}:${WEB_PORT}`;

  // 快速路径：已经确认过 Web Server 在运行
  if (_webReady) {
    return dashboardUrl;
  }

  // 正在启动中：等待已有的 Promise
  if (_startingPromise !== null) {
    await _startingPromise;
    return dashboardUrl;
  }

  // 发起新的启动流程
  _startingPromise = _doEnsureWeb()
    .then(() => {
      _webReady = true;
    })
    .finally(() => {
      _startingPromise = null;
    });

  await _startingPromise;
  return dashboardUrl;
}

async function _doEnsureWeb(): Promise<void> {
  // ============================================
  // 第一步：检查 Web Server 是否已运行
  // 已运行时直接返回（不打开浏览器）
  // ============================================
  if (await isWebRunning()) {
    return;
  }

  // ============================================
  // 第二步：Web Server 未运行，spawn 新进程
  // ============================================
  await spawnWeb();

  // ============================================
  // 第三步：等待 Web Server 就绪
  // ============================================
  const ready = await waitForWebReady();

  if (!ready) {
    throw new Error(
      `ghostcode-web 启动超时（${WEB_START_TIMEOUT_MS}ms），` +
      `请检查 ${WEB_BIN_PATH} 是否存在且可执行`
    );
  }

  // ============================================
  // 第四步：首次启动成功，自动打开浏览器
  // 仅在本次确实启动了新进程时打开（已运行时在第一步就返回了）
  // ============================================
  try {
    await openURL(`http://${WEB_HOST}:${WEB_PORT}`);
  } catch {
    // 浏览器打开失败不阻断流程（headless 环境或 SSH 场景）
    console.error("[GhostCode] 浏览器自动打开失败，请手动访问: " +
      `http://${WEB_HOST}:${WEB_PORT}`);
  }
}

/**
 * 获取 Dashboard URL
 *
 * @returns Dashboard 的完整 URL
 */
export function getWebUrl(): string {
  return `http://${WEB_HOST}:${WEB_PORT}`;
}

/**
 * 获取 Web Server 端口
 *
 * @returns Web Server 端口号
 */
export function getWebPort(): number {
  return WEB_PORT;
}

/**
 * 重置 Web 状态缓存（用于测试）
 */
export function resetWebState(): void {
  _webReady = false;
  _startingPromise = null;
}
