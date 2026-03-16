export { AddrDescriptor, ensureDaemon, getDaemonBinaryPath, startHeartbeat, stopDaemon } from './daemon.js';
export { DaemonError, DaemonRequest, DaemonResponse, IpcConnectionError, IpcProtocolError, IpcTimeoutError, callDaemon, createConnection, resetClient } from './ipc.js';
export { HookEventType, HookHandler, clearHooks, getHooks, registerHook } from './hooks/registry.js';
export { initializeHooks, preToolUseHandler, stopHandler } from './hooks/handlers.js';
import 'node:net';
import './router/streaming.js';

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

/** GhostCode Plugin 版本号 */
declare const VERSION = "0.1.0";
/** GhostCode Plugin 名称 */
declare const PLUGIN_NAME = "ghostcode";

export { PLUGIN_NAME, VERSION };
