/**
 * @file hooks/handlers.ts
 * @description GhostCode Plugin Hook 处理器实现
 *              提供四个核心函数：
 *              - preToolUseHandler: 工具调用前确保 Daemon 已启动并启动心跳，并获取 session lease
 *              - stopHandler: 会话终止时停止心跳，通过引用计数决定是否关闭 Daemon
 *              - userPromptSubmitHandler: 用户提交 prompt 时检测 Magic Keywords 并注入上下文
 *              - initializeHooks: 将上述处理器注册到 Hook 系统
 *
 *              状态管理：
 *              - daemonPromise: 缓存 ensureDaemon 的 Promise，防止重复启动
 *              - stopHeartbeat: 保存心跳停止函数，用于 stopHandler 调用
 *              - leaseManager: Session Lease 引用计数管理器（多会话共享 Daemon）
 *              - currentLeaseId: 当前会话的 lease ID，用于 stopHandler 释放
 * @author Atlas.oi
 * @date 2026-03-04
 */

import { ensureDaemon, stopDaemon, startHeartbeat } from "../daemon.js";
import type { AddrDescriptor } from "../daemon.js";
import { SessionLeaseManager } from "../session-lease.js";
import { join } from "node:path";
import { homedir } from "node:os";
import { registerHook } from "./registry.js";
import { detectMagicKeywords, resolveKeywordPriority } from "../keywords/index.js";
import { writeKeywordState } from "../keywords/state.js";
import type { KeywordState } from "../keywords/types.js";
import { appendSessionContent } from "../learner/manager.js";

// ============================================
// 工作区根目录
// Claude Code Plugin 保证 process.cwd() 等于用户执行 claude 命令的目录（即项目根目录）
// 如果未来需要支持子目录场景，可改为向上查找 .git 或 .ghostcode 目录
// ============================================
const WORKSPACE_ROOT = process.cwd();

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

/**
 * Session Lease 引用计数管理器
 *
 * 作用：管理多会话共享单 Daemon 时的引用计数。
 * 每个会话 acquire 一个 lease，只有最后一个会话 release 时才真正关闭 Daemon。
 * 默认路径: ~/.ghostcode/daemon/sessions.json
 */
const leaseManager = new SessionLeaseManager(
  join(homedir(), ".ghostcode", "daemon", "sessions.json"),
);

/**
 * 当前会话的 lease ID
 *
 * 作用：保存 acquireLease 返回的 leaseId，在 stopHandler 中传给 releaseLease 以识别本会话。
 * 重置：stopHandler 调用后清空。
 */
let currentLeaseId: string | null = null;

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

  // ============================================
  // 分层 try-catch：Daemon 启动与本地状态初始化分离
  // Daemon 启动成功后不应因 lease/heartbeat 异常回退到"未启动"状态
  // ============================================
  let addr: AddrDescriptor;
  try {
    addr = await daemonPromise;
  } catch (err) {
    // ensureDaemon 失败时清空缓存，允许下次重试
    // 记录错误但不阻断工具调用流程
    console.error("[GhostCode] Daemon 启动失败，工具调用将继续但无协作功能:", err);
    daemonPromise = null;
    return;
  }

  // Daemon 已启动成功，注入环境变量（即使后续 lease/heartbeat 失败也保留）
  process.env["GHOSTCODE_SOCKET_PATH"] = addr.path;

  // 心跳和 lease 初始化：失败不影响 Daemon 已启动的事实
  try {
    stopHeartbeat = startHeartbeat(addr);
  } catch {
    // 心跳启动失败不影响核心功能，仅损失自动重连能力
    console.error("[GhostCode] 心跳启动失败，Daemon 仍可正常使用");
  }

  // 获取 session lease（引用计数 +1），记录本会话参与 Daemon 使用
  // 确保只在首次 Daemon 启动时 acquire 一次（幂等保护）
  if (currentLeaseId === null) {
    try {
      const lease = leaseManager.acquireLease();
      currentLeaseId = lease.leaseId;
    } catch {
      // lease 获取失败不影响 Daemon 使用
      // stopHandler 在 currentLeaseId === null 时将检查 refcount 决定是否关闭
      console.error("[GhostCode] Session lease 获取失败，停止时将安全降级");
    }
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

  // ============================================
  // 基于 Session Lease 的安全停止逻辑
  // 多会话共享 Daemon 时，只有最后一个会话退出才真正关闭 Daemon
  //
  // 两种路径：
  // 1. 正常路径（有 leaseId）：release 后检查 isLast
  // 2. 异常路径（无 leaseId，acquire 曾失败）：显式读取 refcount 判断
  //    只有 refcount === 0 才关闭，防止误杀其他会话正在使用的 Daemon
  // ============================================
  let shouldShutdown = false;

  if (currentLeaseId !== null) {
    // 正常路径：本会话持有 lease，释放后由 isLast 决定
    try {
      const result = leaseManager.releaseLease(currentLeaseId);
      shouldShutdown = result.isLast;
    } catch {
      // release 失败：无法确认自己是否为最后一个，保守不关闭
      // 如果确实是孤儿 Daemon，由心跳超时机制或下次启动时的 cleanup 处理
      console.error("[GhostCode] Lease 释放失败，保守保留 Daemon 运行");
    }
    currentLeaseId = null;
  } else {
    // 异常路径：acquire 曾失败，本会话从未持有 lease
    // 显式读取 refcount，只有确认无其他会话时才关闭
    try {
      const refcount = leaseManager.getRefcount();
      shouldShutdown = refcount === 0;
    } catch {
      // 连 refcount 都读不到：无法确认状态，保守不关闭
      console.error("[GhostCode] 无法读取 refcount，保守保留 Daemon 运行");
    }
  }

  if (shouldShutdown) {
    // 关闭 Daemon（幂等，未运行时静默返回）
    await stopDaemon();
  }

  // 触发 Skill Learning 分析（会话结束时提取可复用模式）
  try {
    const { onSessionEnd } = await import("../learner/index.js");
    await onSessionEnd();
  } catch {
    // Skill Learning 失败不阻断 Stop 流程
  }
}

/**
 * Magic Keyword 激活后注入给 Claude 的上下文说明映射
 * 不同模式激活时，向 Claude 说明当前工作上下文
 */
const KEYWORD_CONTEXT_MAP: Record<string, string> = {
  ralph: "[GhostCode] Ralph 验证模式已激活 - 代码变更将经过 7 项自动验证",
  autopilot: "[GhostCode] Autopilot 模式已激活 - 全自动执行模式",
  team: "[GhostCode] Team 模式已激活 - 多 Agent 协作模式",
  ultrawork: "[GhostCode] UltraWork 模式已激活 - 极致工作模式",
};

/**
 * UserPromptSubmit Hook 处理器
 *
 * 业务逻辑：
 * 1. 从事件中防御性提取用户 prompt 文本（事件格式不确定时静默处理）
 * 2. 调用 detectMagicKeywords 检测 Magic Keywords（已内置 sanitize 预处理）
 * 3. 调用 resolveKeywordPriority 取最高优先级关键词
 * 4. cancel 关键词：写入清除状态，返回取消提示
 * 5. 其他关键词：写入激活状态，返回 additionalContext 注入 Claude 上下文
 * 6. 无关键词：返回 undefined，透传不干扰
 *
 * @param event - Hook 事件，预期包含 prompt 字段
 * @returns 无关键词时返回 undefined；有关键词时返回 { additionalContext }
 */
export async function userPromptSubmitHandler(
  event: unknown,
): Promise<{ additionalContext: string } | undefined> {
  // ============================================
  // 第一步：从事件中提取用户 prompt 文本
  // 防御性解析：事件格式不符时静默返回 undefined
  // ============================================
  const prompt =
    typeof event === "object" &&
    event !== null &&
    "prompt" in event
      ? String((event as Record<string, unknown>).prompt)
      : "";

  if (!prompt) {
    return undefined;
  }

  // ============================================
  // C5 修复：将 prompt 内容追加到 Skill Learning 会话缓冲区
  // 确保 onSessionEnd 能够检测到当前会话的所有用户输入模式
  // ============================================
  appendSessionContent(prompt);

  // ============================================
  // 第二步：检测 Magic Keywords
  // detectMagicKeywords 内部已调用 sanitize，自动排除代码块/URL 中的关键词
  // ============================================
  const matches = detectMagicKeywords(prompt);
  const topMatch = resolveKeywordPriority(matches);

  if (topMatch === null) {
    // 无关键词命中，透传不干扰 Claude Code 正常流程
    return undefined;
  }

  // ============================================
  // 第三步：处理 cancel 特殊逻辑
  // cancel 清除激活状态，不进入模式激活流程
  // ============================================
  if (topMatch.type === "cancel") {
    const clearState: KeywordState = {
      active: null,
      activatedAt: null,
      prompt: null,
    };
    try {
      await writeKeywordState(WORKSPACE_ROOT, clearState);
    } catch {
      // 状态文件写入失败不阻断流程，错误已在 writeKeywordState 内部暴露
    }
    return { additionalContext: "[GhostCode] 模式已取消" };
  }

  // ============================================
  // 第四步：激活关键词模式
  // 写入状态文件记录激活信息，返回 additionalContext 告知 Claude
  // ============================================
  const newState: KeywordState = {
    active: topMatch.type,
    activatedAt: new Date().toISOString(),
    prompt,
  };

  try {
    await writeKeywordState(WORKSPACE_ROOT, newState);
  } catch {
    // 状态文件写入失败不阻断流程
  }

  // 构建上下文注入信息，告知 Claude 当前激活的工作模式
  const contextMessage =
    KEYWORD_CONTEXT_MAP[topMatch.type] ??
    `[GhostCode] ${topMatch.type} 模式已激活`;

  return { additionalContext: contextMessage };
}

/**
 * 初始化所有 Hook 处理器
 *
 * 将 preToolUseHandler、stopHandler 和 userPromptSubmitHandler 注册到 Hook 系统。
 * 应在 Plugin 激活（activate）时调用一次。
 */
export function initializeHooks(): void {
  // 注册工具调用前处理器（确保 Daemon 启动）
  registerHook("PreToolUse", preToolUseHandler);
  // 注册会话终止处理器（停止心跳并关闭 Daemon）
  registerHook("Stop", stopHandler);
  // 注册用户 prompt 提交处理器（Magic Keywords 检测 + 状态写入 + 上下文注入）
  registerHook("UserPromptSubmit", userPromptSubmitHandler);
}
