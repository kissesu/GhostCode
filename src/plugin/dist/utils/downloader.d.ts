/**
 * @file downloader.ts
 * @description 从 GitHub Release 下载平台 bundle 的下载器
 *   支持进度回调和指数退避重试，使用 Node.js 内置 fetch API
 * @author Atlas.oi
 * @date 2026-03-04
 */
/**
 * 下载选项
 */
interface DownloadOptions {
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
interface DownloadResult {
    /** 下载后的文件路径 */
    filePath: string;
    /** 总下载字节数 */
    bytesDownloaded: number;
}
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
declare function downloadWithRetry(options: DownloadOptions): Promise<DownloadResult>;

export { type DownloadOptions, type DownloadResult, downloadWithRetry };
