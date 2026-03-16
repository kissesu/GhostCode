/**
 * @file install.ts
 * @description GhostCode Plugin 首次运行安装逻辑
 *   支持两种安装模式：
 *   1. 本地复制模式：从 Plugin 包内的 bin/ 目录复制预编译二进制（离线/快速路径）
 *   2. Release 下载模式：从 GitHub Release 下载平台 bundle，验证 checksum 后解压安装
 *   通过 ~/.ghostcode/.installed 标记文件避免重复安装
 * @author Atlas.oi
 * @date 2026-03-04
 */
/**
 * 根据版本和平台信息生成 GitHub Release 资产的下载 URL
 *
 * URL 格式：
 * https://github.com/{repo}/releases/download/v{version}/ghostcode-{platform}.tar.gz
 *
 * @param version 版本号（不含 v 前缀，如 "0.2.0"）
 * @param platform Node.js process.platform 值（"darwin" | "linux"）
 * @param arch Node.js process.arch 值（"arm64" | "x64"）
 * @returns 完整的 tar.gz 下载 URL
 * @throws Error 不支持的平台组合
 */
declare function buildReleaseAssetUrl(version: string, platform: string, arch: string): string;
/**
 * 从 GitHub Release 下载并安装 GhostCode 二进制
 *
 * 业务逻辑：
 * 1. 检查是否已安装且版本匹配（快速路径，跳过下载）
 * 2. 生成 bundle 下载 URL 和 SHA256SUMS URL
 * 3. 下载 bundle tar.gz 到临时目录
 * 4. 下载 SHA256SUMS 文件
 * 5. 解析 SHA256SUMS，获取对应 bundle 的期望 checksum
 * 6. 验证下载文件 checksum（不匹配则中止，不降级！）
 * 7. 解压 bundle 到目标目录
 * 8. 验证 ghostcoded 和 ghostcode-mcp 均已安装
 * 9. 写入安装标记文件
 *
 * @param version 要安装的版本号（不含 v 前缀，如 "0.2.0"）
 * @param platform Node.js process.platform 值（"darwin" | "linux"）
 * @param arch Node.js process.arch 值（"arm64" | "x64"）
 * @param targetDir 安装目标目录（默认为 ~/.ghostcode/bin）
 * @throws Error checksum 不匹配、下载失败或解压失败时（禁止静默降级）
 */
declare function installFromRelease(version: string, platform: string, arch: string, targetDir?: string): Promise<void>;
/**
 * 执行 GhostCode Plugin 安装（本地复制模式）
 *
 * 业务逻辑：
 * 1. 读取当前 Plugin 版本
 * 2. 检查 bin/ 目录是否已安装且版本匹配（快速路径，跳过安装）
 * 3. 检测当前平台
 * 4. 定位 Plugin 包内对应平台的 ghostcoded 和 ghostcode-mcp 二进制文件
 * 5. 创建目标目录（~/.ghostcode/bin/）
 * 6. 复制双二进制并设置可执行权限
 * 7. 写入安装标记文件到 bin/ 目录（与 installFromRelease 统一路径）
 *
 * @throws Error 当平台不受支持或二进制文件不存在时
 */
declare function installGhostcode(): Promise<void>;

export { buildReleaseAssetUrl, installFromRelease, installGhostcode };
