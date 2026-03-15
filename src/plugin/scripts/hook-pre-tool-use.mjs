/**
 * @file scripts/hook-pre-tool-use.mjs
 * @description PreToolUse Hook 脚本
 *              在每次工具调用前确保 GhostCode Daemon 已启动并获取 Session Lease。
 *
 *              与原 handlers.ts 的关键区别：
 *              - 每次调用是独立进程，使用文件状态替代内存变量
 *              - 不启动心跳（进程执行完即退出，心跳无意义）
 *              - 通过状态文件实现跨调用的幂等性
 *
 *              参考: src/plugin/src/hooks/handlers.ts - preToolUseHandler
 * @author Atlas.oi
 * @date 2026-03-05
 */

import { existsSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { homedir } from "node:os";

// ============================================
// 常量：路径配置
// ============================================

// GhostCode 主目录，优先使用环境变量覆盖
const GHOSTCODE_HOME = process.env.GHOSTCODE_HOME || join(homedir(), ".ghostcode");

// Hook 状态文件：记录 Daemon 启动状态和 Lease 信息
// 所有 Hook 脚本共享此文件，实现跨进程状态传递
const STATE_FILE = join(GHOSTCODE_HOME, "state", "hook-state.json");

// Plugin 根目录（由 run.mjs 注入或从脚本路径推导）
const PLUGIN_ROOT = process.env.CLAUDE_PLUGIN_ROOT || join(dirname(new URL(import.meta.url).pathname), "..");

// ============================================
// 状态文件读写工具函数
// ============================================

/**
 * 读取 Hook 状态文件
 *
 * 业务逻辑：
 * 1. 检查文件是否存在
 * 2. 解析 JSON 内容
 * 3. 文件不存在或解析失败时返回默认空状态
 *
 * @returns {{ daemonStarted: boolean, socketPath: string|null, leaseId: string|null }}
 */
function readState() {
  try {
    if (existsSync(STATE_FILE)) {
      return JSON.parse(readFileSync(STATE_FILE, "utf-8"));
    }
  } catch {
    // 状态文件解析失败时，返回默认状态重新初始化
  }
  return { daemonStarted: false, socketPath: null, leaseId: null, webStarted: false };
}

/**
 * 写入 Hook 状态文件
 *
 * 业务逻辑：
 * 1. 确保父目录存在（首次运行时创建）
 * 2. 将状态序列化为 JSON 写入文件
 *
 * @param {{ daemonStarted: boolean, socketPath: string|null, leaseId: string|null }} state - 要持久化的状态
 */
function writeState(state) {
  // 确保状态目录存在，首次运行时自动创建
  const dir = dirname(STATE_FILE);
  mkdirSync(dir, { recursive: true });
  writeFileSync(STATE_FILE, JSON.stringify(state, null, 2), "utf-8");
}

/**
 * 检测指定 PID 的进程是否存活
 *
 * 使用 kill -0 信号探测进程，不发送实际信号
 *
 * @param {number} pid - 目标进程 PID
 * @returns {boolean} 进程存活返回 true，否则返回 false
 */
function isProcessAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    // 进程不存在或无权限访问，均视为不存活
    return false;
  }
}

// ============================================
// 主逻辑
// ============================================

/**
 * PreToolUse Hook 主函数
 *
 * 业务逻辑说明：
 * 1. 读取当前状态文件，获取 Daemon 启动状态
 * 2. 如果 Daemon 已启动，验证进程是否仍在运行（幂等保护）
 * 3. 如果 Daemon 不存在或进程已退出，重新启动 Daemon
 * 4. 获取 Session Lease（如果尚未持有）
 * 5. 将最新状态写回状态文件
 */
async function main() {
  // ============================================
  // 第一步：读取当前状态
  // ============================================
  const state = readState();

  // ============================================
  // 第二步：检查 Daemon 是否已启动（幂等保护）
  // 如果状态文件记录 Daemon 已启动，通过 addr.json 验证进程是否仍然存活
  // ============================================
  if (state.daemonStarted && state.socketPath) {
    // 读取 addr.json 获取 Daemon 进程信息
    const addrPath = join(GHOSTCODE_HOME, "daemon", "ghostcoded.addr.json");
    let daemonAlive = false;
    try {
      if (existsSync(addrPath)) {
        const addr = JSON.parse(readFileSync(addrPath, "utf-8"));
        if (addr.pid && isProcessAlive(addr.pid)) {
          daemonAlive = true;
        }
      }
    } catch {
      // addr.json 读取失败，说明 Daemon 状态异常，需要重新启动
    }

    if (daemonAlive) {
      // Daemon 仍在运行，但仍需确保 Web Dashboard 也在运行
      // Bug 修复：之前直接 return 导致 ensureWeb 永远不会被调用
      if (!state.webStarted) {
        try {
          const { ensureWeb } = await import(join(PLUGIN_ROOT, "dist", "web.js"));
          await ensureWeb();
          state.webStarted = true;
          writeState(state);
        } catch (err) {
          // Dashboard 启动失败不阻断工具调用
          console.error("[GhostCode] Dashboard 自动启动失败:", err);
        }
      }
      return;
    }

    // Daemon 进程不再存活，重置状态准备重新启动
    state.daemonStarted = false;
    state.socketPath = null;
    state.leaseId = null;
    state.webStarted = false;
  }

  // ============================================
  // 第三步：启动 Daemon
  // 动态导入编译产物，支持 CLAUDE_PLUGIN_ROOT 环境变量覆盖
  // ============================================
  try {
    const { ensureDaemon } = await import(join(PLUGIN_ROOT, "dist", "daemon.js"));
    const addr = await ensureDaemon();

    // Daemon 启动成功，更新状态
    state.daemonStarted = true;
    state.socketPath = addr.path;

    // ============================================
    // 第四步：获取 Session Lease（如果尚未持有）
    // Lease 标识当前 Claude Code 会话，用于多 Agent 协作管理
    // ============================================
    if (!state.leaseId) {
      try {
        const { SessionLeaseManager } = await import(join(PLUGIN_ROOT, "dist", "session-lease.js"));
        // sessions.json 记录所有活跃会话的 Lease 信息
        const sessionsPath = join(GHOSTCODE_HOME, "daemon", "sessions.json");
        const leaseManager = new SessionLeaseManager(sessionsPath);
        const lease = leaseManager.acquireLease();
        state.leaseId = lease.leaseId;
      } catch (err) {
        // Lease 获取失败不阻断流程，仅记录错误
        // 没有 Lease 仍可正常使用，只是部分协作功能受限
        console.error("[GhostCode] Session lease 获取失败:", err);
      }
    }

    // ============================================
    // 第五步：启动 Dashboard Web 服务（单实例保证）
    // ensureWeb() 内部检查 ghostcode-web 是否已运行，
    // 已运行则跳过，未运行则自动启动并等待健康检查通过
    // ============================================
    try {
      const { ensureWeb } = await import(join(PLUGIN_ROOT, "dist", "web.js"));
      await ensureWeb();
      state.webStarted = true;
    } catch (err) {
      // Dashboard 启动失败不阻断工具调用
      // 用户仍可通过 /gc-web 命令手动启动
      console.error("[GhostCode] Dashboard 自动启动失败:", err);
    }

    // ============================================
    // 第六步：将最新状态写回文件
    // 下次 Hook 调用时可从此文件读取状态，实现跨进程幂等性
    // ============================================
    writeState(state);
  } catch (err) {
    // Daemon 启动失败，记录错误但不阻断工具调用
    // 遵循 exit 0 策略：GhostCode 的问题不应影响用户正常使用 Claude Code
    console.error("[GhostCode] Daemon 启动失败，工具调用将继续但无协作功能:", err);

    // 重置状态文件，下次 Hook 调用时会重试启动
    writeState({ daemonStarted: false, socketPath: null, leaseId: null, webStarted: false });
  }
}

// ============================================
// 入口：执行主逻辑
// 任何未捕获的异常都 exit 0，绝不阻断 Claude Code 的工具调用
// ============================================
main().catch((err) => {
  console.error("[GhostCode] hook-pre-tool-use 异常:", err);
  // exit 0 策略：即使 Hook 完全失败，也不影响工具调用继续执行
  process.exit(0);
});
