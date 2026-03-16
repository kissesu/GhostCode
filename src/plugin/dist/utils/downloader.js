import { createWriteStream } from "node:fs";
import { pipeline } from "node:stream/promises";
import { Readable } from "node:stream";
function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
function calcBackoffDelay(attempt, baseDelayMs) {
  const exponentialDelay = baseDelayMs * Math.pow(2, attempt);
  const jitter = exponentialDelay * 0.1 * Math.random();
  return Math.floor(exponentialDelay + jitter);
}
async function fetchToFile(url, destPath, onProgress) {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`HTTP \u4E0B\u8F7D\u5931\u8D25: ${response.status} ${response.statusText} - ${url}`);
  }
  const contentLength = response.headers.get("content-length");
  const total = contentLength ? parseInt(contentLength, 10) : 0;
  if (!response.body) {
    throw new Error(`\u54CD\u5E94\u4F53\u4E3A\u7A7A: ${url}`);
  }
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
      }
    });
    await pipeline(nodeReadable, fileStream);
  } finally {
    reader.releaseLock();
  }
  return bytesDownloaded;
}
async function downloadWithRetry(options) {
  const {
    url,
    destPath,
    onProgress,
    maxRetries = 3,
    baseDelayMs = 1e3
  } = options;
  let lastError;
  for (let attempt = 0; attempt < maxRetries; attempt++) {
    try {
      const bytesDownloaded = await fetchToFile(url, destPath, onProgress);
      return { filePath: destPath, bytesDownloaded };
    } catch (err) {
      lastError = err instanceof Error ? err : new Error(String(err));
      if (attempt < maxRetries - 1) {
        const delayMs = calcBackoffDelay(attempt, baseDelayMs);
        await sleep(delayMs);
      }
    }
  }
  throw lastError ?? new Error(`\u4E0B\u8F7D\u5931\u8D25: ${url}`);
}
export {
  downloadWithRetry
};
//# sourceMappingURL=downloader.js.map