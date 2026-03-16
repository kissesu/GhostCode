/**
 * @file gc-web.ts
 * @description /gc-web Magic Keyword Handler
 *              用户输入 /gc-web 时确保 Dashboard 运行并在浏览器中打开。
 *              复用 ensureWeb() 实现单实例保证：
 *              - 未运行 → 自动启动 + 打开浏览器
 *              - 已运行 → 仅打开浏览器（不重复启动）
 * @author Atlas.oi
 * @date 2026-03-15
 */
/**
 * handleGcWeb 的可选参数
 * 使用依赖注入模式，方便测试时 mock 各依赖项
 */
interface GcWebOptions {
    /** Dashboard web 服务端口，默认 7070（与 ghostcode-web 一致） */
    port?: number;
    /** 会话认证 Token，存在时附加到 URL 查询参数 */
    token?: string;
}
/**
 * 获取 Dashboard URL
 *
 * 业务逻辑：
 * 1. 以 port 参数构建 http://127.0.0.1:<port> 基础 URL
 * 2. 如果提供了 session token，将其附加到 URL 查询参数 ?token=<token>
 *
 * @param port - Dashboard web 服务监听端口
 * @param token - 可选的会话认证 Token
 * @returns 完整的 Dashboard URL 字符串
 */
declare function getDashboardUrl(port: number, token?: string): string;
/**
 * 处理 /gc-web keyword
 *
 * 业务逻辑：
 * 1. 调用 ensureWeb() 确保 Web Server 在运行（单实例保证）
 * 2. 构建 Dashboard URL（携带 token 如果有）
 * 3. 调用 openURL 在默认浏览器中打开
 *
 * 与自动启动的区别：
 * /gc-web 是用户主动触发，总是会打开浏览器（即使 Web Server 已在运行）
 *
 * @param options - 可选配置，支持端口和 Token
 * @returns 成功打开的 Dashboard URL 字符串
 */
declare function handleGcWeb(options?: GcWebOptions): Promise<string>;

export { type GcWebOptions, getDashboardUrl, handleGcWeb };
