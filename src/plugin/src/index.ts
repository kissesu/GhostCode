/**
 * @file GhostCode Plugin 主入口
 * @description GhostCode Claude Code Plugin 的公开 API 导出入口。
 *              作为 TypeScript 薄壳，本文件聚合三个核心模块的导出：
 *              - daemon: Daemon 进程管理（T17 实现）
 *              - ipc: IPC 通信层（T18 实现）
 *              - hooks: Claude Code Hook 注册（后续任务实现）
 *
 *              Plugin 架构设计：
 *              Claude Code 宿主 → Plugin (index.ts) → IPC → Rust Daemon
 *
 *              启动顺序：
 *              1. installGhostcode()：平台检测 + 二进制部署到 ~/.ghostcode/bin/ghostcoded
 *              2. MCP server 初始化（由宿主调用）
 * @author Atlas.oi
 * @date 2026-03-01
 */

// 首次运行安装（平台检测 + 二进制部署到 ~/.ghostcode/bin/ghostcoded）
import { installGhostcode } from "./install.js";
// Hook 初始化（注册 PreToolUse 和 Stop 处理器）
import { initializeHooks } from "./hooks/index.js";

// ============================================
// 首次运行时检测平台并部署 Daemon 二进制
// 必须在 MCP server 启动之前完成安装
// ============================================
await installGhostcode();

// ============================================
// 注册 Claude Code 生命周期 Hook 处理器
// PreToolUse: 工具调用前确保 Daemon 已启动
// Stop: 会话结束时停止心跳并关闭 Daemon
// ============================================
initializeHooks();

// ============================================
// Daemon 管理模块导出（T17 实现）
// ============================================
export type { AddrDescriptor } from "./daemon.js";
export { ensureDaemon, stopDaemon, startHeartbeat, getDaemonBinaryPath } from "./daemon.js";
// ============================================
// IPC 通信模块导出（T18 实现）
// ============================================
export type { DaemonRequest, DaemonError, DaemonResponse } from "./ipc.js";
export {
  callDaemon,
  createConnection,
  resetClient,
  IpcTimeoutError,
  IpcConnectionError,
  IpcProtocolError,
} from "./ipc.js";

// ============================================
// Hook 注册模块导出
// ============================================
export type { HookEventType, HookHandler } from "./hooks/index.js";
export { registerHook, getHooks, clearHooks, initializeHooks, preToolUseHandler, stopHandler } from "./hooks/index.js";

// ============================================
// Plugin 版本信息
// ============================================

/** GhostCode Plugin 版本号 */
export const VERSION = "0.1.0";

/** GhostCode Plugin 名称 */
export const PLUGIN_NAME = "ghostcode";
