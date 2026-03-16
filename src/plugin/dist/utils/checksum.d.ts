/**
 * @file checksum.ts
 * @description SHA256 校验工具，用于验证下载文件的完整性
 *   使用 Node.js 内置 crypto 模块，不引入外部依赖
 * @author Atlas.oi
 * @date 2026-03-04
 */
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
declare function computeSha256(filePath: string): Promise<string>;
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
declare function parseSha256Sums(content: string, fileName: string): string | null;
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
declare function verifyChecksum(filePath: string, expectedHash: string): Promise<boolean>;

export { computeSha256, parseSha256Sums, verifyChecksum };
