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
declare function ensureWeb(): Promise<string>;
/**
 * 获取 Dashboard URL
 *
 * @returns Dashboard 的完整 URL
 */
declare function getWebUrl(): string;
/**
 * 获取 Web Server 端口
 *
 * @returns Web Server 端口号
 */
declare function getWebPort(): number;
/**
 * 重置 Web 状态缓存（用于测试）
 */
declare function resetWebState(): void;

export { ensureWeb, getWebPort, getWebUrl, resetWebState };
