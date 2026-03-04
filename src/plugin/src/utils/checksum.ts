/**
 * @file checksum.ts
 * @description SHA256 校验工具，用于验证下载文件的完整性
 *   使用 Node.js 内置 crypto 模块，不引入外部依赖
 * @author Atlas.oi
 * @date 2026-03-04
 */

import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";

// ============================================
// SHA256 计算
// ============================================

/**
 * 计算文件的 SHA256 哈希值
 *
 * 业务逻辑：
 * 1. 创建文件读取流（避免大文件一次性加载到内存）
 * 2. 流式计算 SHA256 摘要
 * 3. 返回十六进制格式的哈希字符串
 *
 * @param filePath 文件路径
 * @returns SHA256 哈希的十六进制字符串（小写）
 * @throws Error 文件不存在或读取失败时
 */
export async function computeSha256(filePath: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const hash = createHash("sha256");
    const fileStream = createReadStream(filePath);

    fileStream.on("data", (chunk) => {
      hash.update(chunk);
    });

    fileStream.on("end", () => {
      resolve(hash.digest("hex"));
    });

    fileStream.on("error", (err) => {
      reject(new Error(`计算文件 SHA256 失败: ${filePath} - ${err.message}`));
    });
  });
}

// ============================================
// SHA256SUMS 解析
// ============================================

/**
 * 从 SHA256SUMS 文件内容中查找指定文件名的校验和
 *
 * SHA256SUMS 文件格式（标准 sha256sum 输出）：
 * ```
 * abc123...  ghostcode-darwin-arm64.tar.gz
 * def456...  ghostcode-darwin-x64.tar.gz
 * ```
 *
 * @param content SHA256SUMS 文件的文本内容
 * @param fileName 要查找的文件名（不含路径）
 * @returns 找到的 SHA256 哈希字符串，未找到返回 null
 */
export function parseSha256Sums(content: string, fileName: string): string | null {
  const lines = content.split("\n");

  for (const line of lines) {
    // 跳过空行和注释行
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) {
      continue;
    }

    // SHA256SUMS 格式：<hash>  <filename> 或 <hash> <filename>
    // 使用正则匹配：开头是 64 位十六进制字符，后跟空白和文件名
    const match = /^([0-9a-f]{64})\s+(.+)$/i.exec(trimmed);
    if (!match) {
      continue;
    }

    const [, hash, name] = match;
    // 比较文件名（支持带路径前缀的情况，只比较文件名部分）
    const parsedFileName = name?.trim().split("/").pop() ?? "";
    if (parsedFileName === fileName || name?.trim() === fileName) {
      return hash ?? null;
    }
  }

  return null;
}

// ============================================
// 校验验证
// ============================================

/**
 * 验证文件的 SHA256 校验和是否与期望值匹配
 *
 * 业务逻辑：
 * 1. 计算文件的实际 SHA256 哈希
 * 2. 与期望哈希进行比对（大小写不敏感）
 * 3. 匹配返回 true，不匹配返回 false
 *
 * @param filePath 待验证文件的路径
 * @param expectedHash 期望的 SHA256 哈希值（十六进制字符串）
 * @returns true 表示校验和匹配，false 表示不匹配
 * @throws Error 文件不存在或读取失败时（校验和不匹配不抛错，返回 false）
 */
export async function verifyChecksum(filePath: string, expectedHash: string): Promise<boolean> {
  const actualHash = await computeSha256(filePath);
  // 大小写不敏感比较（统一转小写）
  return actualHash.toLowerCase() === expectedHash.toLowerCase();
}
