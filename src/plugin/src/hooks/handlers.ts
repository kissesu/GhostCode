/**
 * @file hooks/handlers.ts
 * @description GhostCode Plugin Hook 处理器实现
 *              提供三个核心函数：
 *              - preToolUseHandler: 工具调用前确保 Daemon 已启动并启动心跳
 *              - stopHandler: 会话终止时停止心跳并关闭 Daemon
 *              - initializeHooks: 将上述处理器注册到 Hook 系统
 *
 *              状态管理：
 *              - daemonPromise: 缓存 ensureDaemon 的 Promise，防止重复启动
 *              - stopHeartbeat: 保存心跳停止函数，用于 stopHandler 调用
 * @author Atlas.oi
 * @date 2026-03-02
 */

import { ensureDaemon, stopDaemon, startHeartbeat } from "../daemon.js";
import type { AddrDescriptor } from "../daemon.js";
import { registerHook } from "./registry.js";

// ============================================
// 模块级状态（单例保护）
// ============================================

/**
 * 缓存 ensureDaemon 的结果 Promise
 *
 * 作用：首次调用后缓存结果，后续调用直接复用，避免重复启动 Daemon。
 * 重置：stopHandler 调用后清空，确保下次调用重新触发 ensureDaemon。
 */
let daemonPromise: Promise<AddrDescriptor> | null = null;

/**
 * 当前心跳的停止函数
 *
 * 作用：保存 startHeartbeat 返回的停止函数，在 stopHandler 中调用以停止心跳。
 * 重置：stopHandler 调用后清空。
 */
let stopHeartbeat: (() => void) | null = null;

// ============================================
// Hook 处理器实现
// ============================================

/**
 * PreToolUse Hook 处理器
 *
 * 业务逻辑：
 * 1. 如果已有缓存的 Daemon Promise，直接返回（幂等）
 * 2. 调用 ensureDaemon() 确保 Daemon 已启动
 * 3. Daemon 启动成功后，调用 startHeartbeat() 启动心跳监控
 * 4. 若 ensureDaemon 失败，静默处理（不阻断工具调用流程）
 *
 * @param _event - Hook 事件（未使用，符合 HookHandler 类型签名）
 */
export async function preToolUseHandler(_event: unknown): Promise<void> {
  // 已有缓存，直接复用（防止重复启动 Daemon）
  if (daemonPromise !== null) {
    return;
  }

  // 缓存 Promise，防止并发调用触发多次 ensureDaemon
  daemonPromise = ensureDaemon();

  try {
    const addr = await daemonPromise;
    // Daemon 启动成功，开始心跳监控
    stopHeartbeat = startHeartbeat(addr);
  } catch (err) {
    // ensureDaemon 失败时清空缓存，允许下次重试
    // 记录错误但不阻断工具调用流程
    console.error("[GhostCode] Daemon 启动失败，工具调用将继续但无协作功能:", err);
    daemonPromise = null;
  }
}

/**
 * Stop Hook 处理器
 *
 * 业务逻辑：
 * 1. 调用心跳停止函数（如果心跳正在运行）
 * 2. 调用 stopDaemon() 关闭 Daemon
 * 3. 重置所有模块状态（为下次启动做准备）
 *
 * @param _event - Hook 事件（未使用，符合 HookHandler 类型签名）
 */
export async function stopHandler(_event: unknown): Promise<void> {
  // 停止心跳（如果正在运行）
  if (stopHeartbeat !== null) {
    stopHeartbeat();
    stopHeartbeat = null;
  }

  // 重置 Daemon 缓存，下次调用 preToolUseHandler 会重新触发 ensureDaemon
  daemonPromise = null;

  // 关闭 Daemon（幂等，未运行时静默返回）
  await stopDaemon();
}

/**
 * 初始化所有 Hook 处理器
 *
 * 将 preToolUseHandler 和 stopHandler 注册到 Hook 系统。
 * 应在 Plugin 激活（activate）时调用一次。
 */
export function initializeHooks(): void {
  // 注册工具调用前处理器（确保 Daemon 启动）
  registerHook("PreToolUse", preToolUseHandler);
  // 注册会话终止处理器（停止心跳并关闭 Daemon）
  registerHook("Stop", stopHandler);
}
