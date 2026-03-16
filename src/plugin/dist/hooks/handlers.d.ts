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
declare function preToolUseHandler(_event: unknown): Promise<void>;
/**
 * Stop Hook 处理器
 *
 * 业务逻辑（顺序严格固定）：
 * 1. 停止心跳（如果心跳正在运行）
 * 2. 调用 onSessionEnd()（Skill Learning，此时 Daemon 仍在运行，确保分析能访问 Daemon 状态）
 * 3. 调用 releaseLease()（引用计数 -1，决定是否为最后一个会话）
 * 4. 调用 stopDaemon()（仅 isLast=true 时，关闭 Daemon）
 * 5. 重置所有模块状态（为下次启动做准备）
 *
 * 顺序约束：
 * - onSessionEnd 必须在 stopDaemon 之前执行，确保 Daemon 仍在运行时完成 Skill Learning
 * - onSessionEnd 失败不影响后续 releaseLease 和 stopDaemon（隔离错误边界）
 *
 * @param _event - Hook 事件（未使用，符合 HookHandler 类型签名）
 */
declare function stopHandler(_event: unknown): Promise<void>;
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
declare function userPromptSubmitHandler(event: unknown): Promise<{
    additionalContext: string;
} | undefined>;
/**
 * 初始化所有 Hook 处理器
 *
 * 将 preToolUseHandler、stopHandler 和 userPromptSubmitHandler 注册到 Hook 系统。
 * 应在 Plugin 激活（activate）时调用一次。
 */
declare function initializeHooks(): void;

export { initializeHooks, preToolUseHandler, stopHandler, userPromptSubmitHandler };
