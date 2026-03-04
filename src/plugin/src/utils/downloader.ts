/**
 * @file downloader.ts
 * @description 从 GitHub Release 下载平台 bundle 的下载器
 *   支持进度回调和指数退避重试，使用 Node.js 内置 fetch API
 * @author Atlas.oi
 * @date 2026-03-04
 */

import { createWriteStream } from "node:fs";
import { pipeline } from "node:stream/promises";
import { Readable } from "node:stream";

// ============================================
// 类型定义
// ============================================

/**
 * 下载选项
 */
export interface DownloadOptions {
  /** 下载源 URL */
  url: string;
  /** 下载目标路径（本地文件） */
  destPath: string;
  /** 下载进度回调，参数为已下载字节数和总字节数 */
  onProgress?: (downloaded: number, total: number) => void;
  /** 最大重试次数，默认 3 */
  maxRetries?: number;
  /** 指数退避基础延迟（毫秒），默认 1000 */
  baseDelayMs?: number;
}

/**
 * 下载结果
 */
export interface DownloadResult {
  /** 下载后的文件路径 */
  filePath: string;
  /** 总下载字节数 */
  bytesDownloaded: number;
}

// ============================================
// 内部辅助函数
// ============================================

/**
 * 等待指定毫秒数（用于重试延迟）
 *
 * @param ms 等待毫秒数
 */
function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * 计算指数退避延迟时间
 * 公式：baseDelayMs * 2^attempt，带随机抖动避免雷群效应
 *
 * @param attempt 当前重试次数（从 0 开始）
 * @param baseDelayMs 基础延迟毫秒数
 * @returns 延迟毫秒数
 */
function calcBackoffDelay(attempt: number, baseDelayMs: number): number {
  // 指数退避：1s, 2s, 4s...
  const exponentialDelay = baseDelayMs * Math.pow(2, attempt);
  // 加入 10% 随机抖动，避免多客户端同时重试造成雷群效应
  const jitter = exponentialDelay * 0.1 * Math.random();
  return Math.floor(exponentialDelay + jitter);
}

/**
 * 执行单次 HTTP 下载，将响应体流式写入目标文件
 *
 * 业务逻辑：
 * 1. 发起 fetch 请求
 * 2. 检查 HTTP 状态码（非 2xx 抛出错误）
 * 3. 流式写入文件，同时计算进度
 *
 * @param url 下载 URL
 * @param destPath 目标文件路径
 * @param onProgress 进度回调
 * @returns 下载字节数
 * @throws Error HTTP 请求失败或写入失败时
 */
async function fetchToFile(
  url: string,
  destPath: string,
  onProgress?: (downloaded: number, total: number) => void
): Promise<number> {
  const response = await fetch(url);

  if (!response.ok) {
    throw new Error(`HTTP 下载失败: ${response.status} ${response.statusText} - ${url}`);
  }

  // 获取内容长度（用于进度计算，可能为 null）
  const contentLength = response.headers.get("content-length");
  const total = contentLength ? parseInt(contentLength, 10) : 0;

  if (!response.body) {
    throw new Error(`响应体为空: ${url}`);
  }

  // ============================================
  // 流式写入文件，同时追踪下载进度
  // ============================================
  let bytesDownloaded = 0;
  const fileStream = createWriteStream(destPath);

  const reader = response.body.getReader();

  try {
    const nodeReadable = new Readable({
      async read() {
        const { done, value } = await reader.read();
        if (done) {
          this.push(null);
          return;
        }
        bytesDownloaded += value.length;
        if (onProgress) {
          onProgress(bytesDownloaded, total);
        }
        this.push(Buffer.from(value));
      },
    });

    await pipeline(nodeReadable, fileStream);
  } finally {
    reader.releaseLock();
  }

  return bytesDownloaded;
}

// ============================================
// 主要导出函数
// ============================================

/**
 * 下载文件，支持指数退避重试
 *
 * 业务逻辑：
 * 1. 尝试发起下载请求
 * 2. 若失败，按指数退避策略等待后重试
 * 3. 超过最大重试次数则抛出最后一次错误
 * 4. 成功后返回下载结果
 *
 * @param options 下载选项
 * @returns 下载结果（文件路径和字节数）
 * @throws Error 超过最大重试次数后仍失败时
 */
export async function downloadWithRetry(options: DownloadOptions): Promise<DownloadResult> {
  const {
    url,
    destPath,
    onProgress,
    maxRetries = 3,
    baseDelayMs = 1000,
  } = options;

  let lastError: Error | undefined;

  for (let attempt = 0; attempt < maxRetries; attempt++) {
    try {
      // ============================================
      // 第一次尝试或重试：发起下载
      // ============================================
      const bytesDownloaded = await fetchToFile(url, destPath, onProgress);
      return { filePath: destPath, bytesDownloaded };
    } catch (err) {
      lastError = err instanceof Error ? err : new Error(String(err));

      // 若还有重试次数，等待后重试
      if (attempt < maxRetries - 1) {
        const delayMs = calcBackoffDelay(attempt, baseDelayMs);
        await sleep(delayMs);
      }
    }
  }

  // 超过最大重试次数，抛出最后一次错误
  throw lastError ?? new Error(`下载失败: ${url}`);
}
