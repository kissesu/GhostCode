/**
 * @file gc-web.ts
 * @description /gc-web Magic Keyword Handler
 * 用户输入 /gc-web 时启动 Dashboard 并在浏览器中打开
 * 支持携带认证 Token，自动检查 Dashboard 运行状态
 * @author Atlas.oi
 * @date 2026-03-04
 */

import { openURL } from "../utils/browser.js";

// ============================================
// 类型定义
// ============================================

/**
 * handleGcWeb 的可选参数
 * 使用依赖注入模式，方便测试时 mock 各依赖项
 */
export interface GcWebOptions {
  /** Dashboard web 服务端口，默认 3000 */
  port?: number;
  /** 会话认证 Token，存在时附加到 URL 查询参数 */
  token?: string;
  /** 检查 Dashboard 是否正在运行的函数（可注入用于测试） */
  checkRunning?: () => Promise<boolean>;
}

// ============================================
// 默认配置常量
// ============================================

/** Dashboard 默认监听端口 */
const DEFAULT_DASHBOARD_PORT = 3000;

// ============================================
// 核心功能实现
// ============================================

/**
 * 获取 Dashboard URL
 *
 * 业务逻辑：
 * 1. 以 port 参数构建 http://localhost:<port> 基础 URL
 * 2. 如果提供了 session token，将其附加到 URL 查询参数 ?token=<token>
 *
 * @param port - Dashboard web 服务监听端口
 * @param token - 可选的会话认证 Token
 * @returns 完整的 Dashboard URL 字符串
 */
export function getDashboardUrl(port: number, token?: string): string {
  // 构建基础 URL
  const baseUrl = `http://localhost:${port}`;

  // 如果有 token，附加到查询参数
  if (token !== undefined && token.length > 0) {
    return `${baseUrl}?token=${encodeURIComponent(token)}`;
  }

  return baseUrl;
}

/**
 * 处理 /gc-web keyword
 *
 * 业务逻辑：
 * 1. 检查 Dashboard 是否正在运行（通过可注入的 checkRunning 函数）
 * 2. 未运行则抛出错误并提示用户启动方式
 * 3. 获取 Dashboard URL（携带 token 如果有）
 * 4. 调用 openURL 在默认浏览器中打开
 *
 * @param options - 可选配置，支持端口、Token 和运行状态检查函数注入
 * @returns 成功打开的 Dashboard URL 字符串
 * @throws Dashboard 未运行时抛出包含 "Dashboard 未运行" 的错误
 */
export async function handleGcWeb(options?: GcWebOptions): Promise<string> {
  const port = options?.port ?? DEFAULT_DASHBOARD_PORT;
  const token = options?.token;
  const checkRunning = options?.checkRunning ?? defaultCheckRunning;

  // ============================================
  // 第一步：检查 Dashboard 运行状态
  // 通过 daemon IPC 查询或注入的检查函数验证
  // ============================================
  const isRunning = await checkRunning();

  if (!isRunning) {
    throw new Error(
      "Dashboard 未运行，请先启动 ghostcode-web 服务。" +
        "可运行: ghostcode web start"
    );
  }

  // ============================================
  // 第二步：构建 Dashboard URL
  // 包含端口信息和可选的认证 Token
  // ============================================
  const url = getDashboardUrl(port, token);

  // ============================================
  // 第三步：在默认浏览器中打开 Dashboard
  // 跨平台支持：macOS (open) / Linux (xdg-open) / Windows (start)
  // ============================================
  await openURL(url);

  return url;
}

// ============================================
// 默认实现：检查 Dashboard 运行状态
// 生产环境中通过 HTTP 健康检查端点验证
// ============================================

/**
 * 默认的 Dashboard 运行状态检查
 *
 * 业务逻辑：
 * 1. 尝试请求 Dashboard 的 /health 端点
 * 2. 返回 200 状态码则认为 Dashboard 正在运行
 * 3. 请求失败（连接拒绝等）则认为未运行
 *
 * @returns Dashboard 是否正在运行
 */
async function defaultCheckRunning(): Promise<boolean> {
  const port = DEFAULT_DASHBOARD_PORT;

  try {
    const response = await fetch(`http://localhost:${port}/health`, {
      // 设置较短的超时，避免长时间等待
      signal: AbortSignal.timeout(2000),
    });
    return response.ok;
  } catch {
    // 连接失败、超时等情况均视为未运行
    return false;
  }
}
