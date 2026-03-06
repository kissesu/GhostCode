/**
 * @file scripts/hook-stop.mjs
 * @description Stop Hook 脚本
 *              在 Claude Code 会话终止时执行资源清理（顺序严格固定）：
 *              1. 触发 Skill Learning 分析（onSessionEnd，此时 Daemon 仍在运行）
 *              2. 释放 Session Lease（引用计数 -1，决定是否为最后一个会话）
 *              3. 如果是最后一个会话，关闭 Daemon（stopDaemon）
 *              4. 清理状态文件（clearState）
 *
 *              顺序约束：onSessionEnd 必须在 stopDaemon 之前执行，确保 Daemon 仍在运行时完成分析
 *
 *              参考: src/plugin/src/hooks/handlers.ts - stopHandler
 * @author Atlas.oi
 * @date 2026-03-05
 */

import { existsSync, readFileSync, writeFileSync, unlinkSync, mkdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { homedir } from "node:os";

// ============================================
// 常量
// ============================================

const GHOSTCODE_HOME = process.env.GHOSTCODE_HOME || join(homedir(), ".ghostcode");

// Hook 状态文件（与 hook-pre-tool-use.mjs 共享）
const STATE_FILE = join(GHOSTCODE_HOME, "state", "hook-state.json");

// Plugin 根目录
const PLUGIN_ROOT = process.env.CLAUDE_PLUGIN_ROOT || join(dirname(new URL(import.meta.url).pathname), "..");

// ============================================
// 状态文件读取
// ============================================

/**
 * 读取 Hook 状态文件
 */
function readState() {
  try {
    if (existsSync(STATE_FILE)) {
      return JSON.parse(readFileSync(STATE_FILE, "utf-8"));
    }
  } catch {
    // 解析失败返回默认状态
  }
  return { daemonStarted: false, socketPath: null, leaseId: null };
}

/**
 * 清理状态文件（会话结束，重置所有状态）
 */
function clearState() {
  try {
    if (existsSync(STATE_FILE)) {
      unlinkSync(STATE_FILE);
    }
  } catch {
    // 清理失败不影响主流程
  }
}

// ============================================
// 主逻辑
// ============================================

async function main() {
  const state = readState();
  let shouldShutdown = false;

  // ============================================
  // 第一步：触发 Skill Learning 分析（onSessionEnd）
  // 必须在 releaseLease 和 stopDaemon 之前调用
  // 确保此时 Daemon 仍在运行，Skill Learning 可以访问 Daemon 状态
  // onSessionEnd 失败不阻断后续 Stop 流程（隔离错误边界）
  // ============================================
  try {
    const { onSessionEnd } = await import(join(PLUGIN_ROOT, "dist", "learner", "index.js"));
    await onSessionEnd();
  } catch (err) {
    // Skill Learning 失败不阻断 Stop 流程
    console.error("[GhostCode] Skill Learning 分析失败，继续执行 Stop 流程:", err);
  }

  // ============================================
  // 第二步：释放 Session Lease（releaseLease）
  // 基于引用计数决定是否需要关闭 Daemon
  // ============================================
  try {
    const { SessionLeaseManager } = await import(join(PLUGIN_ROOT, "dist", "session-lease.js"));
    const sessionsPath = join(GHOSTCODE_HOME, "daemon", "sessions.json");
    const leaseManager = new SessionLeaseManager(sessionsPath);

    if (state.leaseId) {
      // 正常路径：本会话持有 lease，释放后由 isLast 决定
      try {
        const result = leaseManager.releaseLease(state.leaseId);
        shouldShutdown = result.isLast;
      } catch {
        console.error("[GhostCode] Lease 释放失败，保守保留 Daemon 运行");
      }
    } else {
      // 异常路径：acquire 曾失败，本会话从未持有 lease
      // 显式读取 refcount，只有确认无其他会话时才关闭
      try {
        const refcount = leaseManager.getRefcount();
        shouldShutdown = refcount === 0;
      } catch {
        console.error("[GhostCode] 无法读取 refcount，保守保留 Daemon 运行");
      }
    }
  } catch (err) {
    console.error("[GhostCode] Session Lease 模块加载失败:", err);
  }

  // ============================================
  // 第三步：关闭 Daemon（仅最后一个会话时）
  // onSessionEnd 已完成，此时安全关闭 Daemon
  // ============================================
  if (shouldShutdown) {
    try {
      const { stopDaemon } = await import(join(PLUGIN_ROOT, "dist", "daemon.js"));
      await stopDaemon();
    } catch (err) {
      console.error("[GhostCode] Daemon 关闭失败:", err);
    }
  }

  // ============================================
  // 第四步：清理状态文件（clearState）
  // 会话已结束，重置所有 Hook 状态
  // ============================================
  clearState();
}

// 执行主逻辑
main().catch((err) => {
  console.error("[GhostCode] hook-stop 异常:", err);
  // 确保即使异常也清理状态文件
  clearState();
  process.exit(0);
});
