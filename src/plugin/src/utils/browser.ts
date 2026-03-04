/**
 * @file browser.ts
 * @description 跨平台浏览器启动工具
 * 在用户默认浏览器中打开指定 URL，支持 macOS/Linux/Windows
 * @author Atlas.oi
 * @date 2026-03-04
 */

import { execFile } from "child_process";

// ============================================
// 平台命令映射类型定义
// ============================================

/**
 * execFile 回调函数类型
 * 与 child_process.execFile 的回调签名一致
 */
type ExecCallback = (
  error: Error | null,
  stdout: string,
  stderr: string
) => void;

/**
 * 可注入的 execFile 函数类型
 * 用于测试中 mock 底层命令执行，保持业务逻辑可测试性
 */
export type ExecFileFn = (
  command: string,
  args: string[],
  callback: ExecCallback
) => unknown;

// ============================================
// 平台命令映射表
// darwin: open, linux: xdg-open, win32: start
// ============================================

/**
 * 根据当前平台获取浏览器启动命令
 *
 * 平台映射：
 * - darwin (macOS): open <url>
 * - linux: xdg-open <url>
 * - win32 (Windows): start <url>
 *
 * @returns 平台对应的启动命令
 * @throws 不支持的操作系统时抛出错误
 */
function getPlatformCommand(): string {
  const platform = process.platform;

  if (platform === "darwin") {
    return "open";
  } else if (platform === "linux") {
    return "xdg-open";
  } else if (platform === "win32") {
    return "start";
  } else {
    throw new Error(`不支持的操作系统: ${platform}`);
  }
}

/**
 * 使用可注入的 execFile 函数打开 URL（供测试使用）
 *
 * 业务逻辑：
 * 1. 获取当前平台对应的启动命令
 * 2. 调用传入的 execFn 执行命令
 * 3. 等待命令执行完成
 *
 * @param url - 要打开的 URL
 * @param execFn - 可注入的命令执行函数，默认使用 child_process.execFile
 */
export function openURLWithExec(
  url: string,
  execFn: ExecFileFn = execFile
): Promise<void> {
  return new Promise((resolve, reject) => {
    const command = getPlatformCommand();

    execFn(command, [url], (error) => {
      if (error !== null) {
        reject(
          new Error(`浏览器启动失败 [${command}]: ${error.message}`)
        );
      } else {
        resolve();
      }
    });
  });
}

/**
 * 在默认浏览器中打开 URL
 *
 * 平台映射：
 * - darwin: open <url>
 * - linux: xdg-open <url>
 * - win32: start <url>
 *
 * @param url - 要在浏览器中打开的完整 URL
 * @throws 不支持的操作系统或命令执行失败时抛出错误
 */
export async function openURL(url: string): Promise<void> {
  await openURLWithExec(url);
}
