/**
 * @file Hook 注册表
 * @description Hook 系统的核心注册逻辑，独立于处理器实现。
 *              将注册表从 index.ts 中分离，消除 handlers.ts ↔ index.ts 的循环依赖。
 *              handlers.ts 和 index.ts 都从本模块导入，形成单向依赖链。
 * @author Atlas.oi
 * @date 2026-03-02
 */

// Hook 事件类型枚举（与 Claude Code Plugin 协议对应）
export type HookEventType =
  | "PreToolUse"
  | "PostToolUse"
  | "Notification"
  | "Stop";

// Hook 处理函数类型
export type HookHandler = (event: unknown) => Promise<unknown> | unknown;

// 已注册的 Hook 映射表
// Key: HookEventType，Value: 处理函数列表
const registeredHooks = new Map<HookEventType, HookHandler[]>();

/**
 * 注册一个 Hook 处理函数
 *
 * 在 Plugin 初始化时由 initializeHooks() 调用，
 * 将生命周期处理器注册到对应的事件类型。
 *
 * @param eventType Hook 事件类型
 * @param handler 处理函数
 */
export function registerHook(
  eventType: HookEventType,
  handler: HookHandler,
): void {
  const existing = registeredHooks.get(eventType) ?? [];
  registeredHooks.set(eventType, [...existing, handler]);
}

/**
 * 获取指定类型的所有已注册 Hook 处理函数
 *
 * @param eventType Hook 事件类型
 * @returns 处理函数列表
 */
export function getHooks(eventType: HookEventType): HookHandler[] {
  return registeredHooks.get(eventType) ?? [];
}

/**
 * 清除所有已注册的 Hook（主要用于测试）
 */
export function clearHooks(): void {
  registeredHooks.clear();
}
