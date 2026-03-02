/**
 * @file Claude Code Hook 模块统一导出
 * @description GhostCode Plugin 的 Hook 系统公开 API。
 *              本文件仅作为统一导出入口，不包含任何逻辑实现。
 *              注册表逻辑在 registry.ts，处理器逻辑在 handlers.ts。
 *
 *              依赖方向（无循环）：
 *              index.ts → registry.ts（类型 + 注册表）
 *              index.ts → handlers.ts（处理器 + initializeHooks）
 *              handlers.ts → registry.ts（调用 registerHook）
 * @author Atlas.oi
 * @date 2026-03-02
 */

// ============================================
// 注册表导出（类型 + 注册/查询/清除 API）
// ============================================
export type { HookEventType, HookHandler } from "./registry.js";
export { registerHook, getHooks, clearHooks } from "./registry.js";

// ============================================
// 处理器导出（initializeHooks + 具体处理器）
// ============================================
export { initializeHooks, preToolUseHandler, stopHandler } from "./handlers.js";
