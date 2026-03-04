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

import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
  chmodSync,
  unlinkSync,
} from "node:fs";
import { dirname, join, basename } from "node:path";
import { homedir, tmpdir } from "node:os";
import { createRequire } from "node:module";
import { createGunzip } from "node:zlib";
import { createReadStream } from "node:fs";

import { downloadWithRetry } from "./utils/downloader.js";
import { verifyChecksum, parseSha256Sums } from "./utils/checksum.js";

// ============================================
// 常量定义
// ============================================

/** GhostCode 主目录 */
const GHOSTCODE_HOME = join(homedir(), ".ghostcode");

/** Daemon 二进制安装目标路径 */
const DAEMON_BIN_PATH = join(GHOSTCODE_HOME, "bin", "ghostcoded");


/** 安装标记文件路径（记录已安装的版本） */
const INSTALLED_MARKER_PATH = join(GHOSTCODE_HOME, ".installed");

/** GitHub Release 仓库地址（用于下载 bundle） */
const GITHUB_REPO = "kissesu/GhostCode";

// ============================================
// 类型定义
// ============================================

/**
 * 支持的平台类型
 * 对应 bin/ 目录下的三个预编译二进制
 */
type SupportedPlatform =
  | "darwin-arm64"   // macOS Apple Silicon
  | "darwin-x64"     // macOS Intel
  | "linux-x64";     // Linux x86_64

/**
 * 安装标记文件的内容结构
 */
interface InstalledMarker {
  /** 已安装的 Plugin 版本 */
  version: string;
  /** 安装时间（ISO 8601） */
  installedAt: string;
  /** 安装的平台 */
  platform: SupportedPlatform;
}

// ============================================
// 平台识别辅助函数
// ============================================

/**
 * 检测当前运行平台，返回对应的预编译二进制文件名
 *
 * 业务逻辑：
 * 1. 读取 process.platform 和 process.arch
 * 2. 映射到 bin/ 目录下对应的文件名
 * 3. 不支持的平台抛出错误（禁止降级策略）
 *
 * @returns 对应平台的 SupportedPlatform 标识
 * @throws Error 当运行平台不在支持列表中时
 */
function detectPlatform(): SupportedPlatform {
  const { platform, arch } = process;

  if (platform === "darwin" && arch === "arm64") {
    return "darwin-arm64";
  }
  if (platform === "darwin" && (arch === "x64" || arch === "ia32")) {
    return "darwin-x64";
  }
  if (platform === "linux" && arch === "x64") {
    return "linux-x64";
  }

  // 不支持的平台直接报错，不做降级处理
  throw new Error(
    `不支持的平台: ${platform}/${arch}。` +
    `GhostCode 当前支持: macOS ARM64、macOS x64、Linux x64`
  );
}

/**
 * 将平台信息组合为 SupportedPlatform 标识
 *
 * @param platform Node.js process.platform 值
 * @param arch Node.js process.arch 值
 * @returns SupportedPlatform 标识
 * @throws Error 不支持的平台组合
 */
function resolvePlatform(platform: string, arch: string): SupportedPlatform {
  if (platform === "darwin" && arch === "arm64") {
    return "darwin-arm64";
  }
  if (platform === "darwin" && (arch === "x64" || arch === "ia32")) {
    return "darwin-x64";
  }
  if (platform === "linux" && arch === "x64") {
    return "linux-x64";
  }
  throw new Error(
    `不支持的平台: ${platform}/${arch}。` +
    `GhostCode 当前支持: macOS ARM64、macOS x64、Linux x64`
  );
}

/**
 * 将 SupportedPlatform 映射到 bin/ 目录下的文件名
 *
 * @param platform 平台标识
 * @returns bin/ 目录下对应的二进制文件名
 */
function platformToBinaryName(platform: SupportedPlatform): string {
  const mapping: Record<SupportedPlatform, string> = {
    "darwin-arm64": "ghostcoded-darwin-arm64",
    "darwin-x64":   "ghostcoded-darwin-x64",
    "linux-x64":    "ghostcoded-linux-x64",
  };
  return mapping[platform];
}

// ============================================
// 版本检测辅助函数
// ============================================

/**
 * 读取 Plugin 自身的版本号
 * 通过 createRequire 读取 package.json（ESM 兼容）
 *
 * @returns 版本字符串（如 "0.1.0"）
 */
function readPluginVersion(): string {
  // ESM 中使用 createRequire 读取 JSON 文件
  // package.json 必须存在，否则是安装异常，应直接报错（禁止降级回退策略）
  const require = createRequire(import.meta.url);
  const pkg = require("../package.json") as { version: string };
  if (typeof pkg.version !== "string" || !pkg.version) {
    throw new Error("package.json 中缺少有效的 version 字段");
  }
  return pkg.version;
}

/**
 * 检查在 targetDir 中是否已安装且版本匹配
 *
 * 业务逻辑：
 * 1. 读取 targetDir/.installed 标记文件
 * 2. 解析 JSON，比对版本号
 * 3. 验证 ghostcoded 和 ghostcode-mcp 二进制文件是否仍然存在
 *
 * @param currentVersion 当前 Plugin 版本
 * @param targetDir 安装目标目录（默认为 ~/.ghostcode/bin）
 * @returns true 表示已安装且无需重新安装
 */
function isInstalledInDir(currentVersion: string, targetDir: string): boolean {
  const markerPath = join(targetDir, ".installed");

  if (!existsSync(markerPath)) {
    return false;
  }

  // 验证 ghostcoded 和 ghostcode-mcp 二进制文件是否都存在
  // 只检查 daemon 会导致半安装状态被误判为成功
  if (!existsSync(join(targetDir, "ghostcoded"))) {
    return false;
  }
  if (!existsSync(join(targetDir, "ghostcode-mcp"))) {
    return false;
  }

  try {
    const content = readFileSync(markerPath, "utf-8");
    const marker = JSON.parse(content) as InstalledMarker;
    // 版本匹配时跳过安装
    return marker.version === currentVersion;
  } catch {
    // 标记文件损坏，重新安装
    return false;
  }
}

/**
 * 检查在默认路径中是否已安装且版本匹配（向后兼容）
 *
 * @param currentVersion 当前 Plugin 版本
 * @returns true 表示已安装且无需重新安装
 */
function isAlreadyInstalled(currentVersion: string): boolean {
  if (!existsSync(INSTALLED_MARKER_PATH)) {
    return false;
  }

  // 验证二进制文件是否存在（标记文件存在但二进制被删除的情况）
  if (!existsSync(DAEMON_BIN_PATH)) {
    return false;
  }

  try {
    const content = readFileSync(INSTALLED_MARKER_PATH, "utf-8");
    const marker = JSON.parse(content) as InstalledMarker;
    // 版本匹配时跳过安装
    return marker.version === currentVersion;
  } catch {
    // 标记文件损坏，重新安装
    return false;
  }
}

// ============================================
// 标记文件写入
// ============================================

/**
 * 写入安装标记文件（默认路径）
 *
 * @param version 安装的版本号
 * @param platform 安装的平台
 */
function writeInstalledMarker(version: string, platform: SupportedPlatform): void {
  const marker: InstalledMarker = {
    version,
    installedAt: new Date().toISOString(),
    platform,
  };
  writeFileSync(INSTALLED_MARKER_PATH, JSON.stringify(marker, null, 2), "utf-8");
}

/**
 * 写入安装标记文件到指定目录
 *
 * @param version 安装的版本号
 * @param platform 安装的平台
 * @param targetDir 目标目录
 */
function writeInstalledMarkerToDir(version: string, platform: SupportedPlatform, targetDir: string): void {
  const marker: InstalledMarker = {
    version,
    installedAt: new Date().toISOString(),
    platform,
  };
  writeFileSync(join(targetDir, ".installed"), JSON.stringify(marker, null, 2), "utf-8");
}

// ============================================
// Release URL 生成（供测试使用，导出）
// ============================================

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
export function buildReleaseAssetUrl(version: string, platform: string, arch: string): string {
  // 解析为 SupportedPlatform 标识（同时验证平台合法性）
  const supportedPlatform = resolvePlatform(platform, arch);
  const bundleName = `ghostcode-${supportedPlatform}.tar.gz`;
  return `https://github.com/${GITHUB_REPO}/releases/download/v${version}/${bundleName}`;
}

/**
 * 生成 SHA256SUMS 文件的下载 URL
 *
 * @param version 版本号（不含 v 前缀）
 * @returns SHA256SUMS 文件的下载 URL
 */
function buildChecksumUrl(version: string): string {
  // 注意：release-checksums.sh 和 release.yml 产出的文件名是 ghostcode_SHA256SUMS
  // 必须与发布脚本保持一致
  return `https://github.com/${GITHUB_REPO}/releases/download/v${version}/ghostcode_SHA256SUMS`;
}

// ============================================
// Bundle 解压逻辑
// ============================================

/**
 * 解压 tar.gz bundle 到目标目录
 *
 * tar.gz 的内部结构预期为：
 * ghostcoded        (Daemon 二进制)
 * ghostcode-mcp     (MCP 代理二进制)
 *
 * 业务逻辑：
 * 1. 使用 Node.js 内置 zlib 解压 gzip 层
 * 2. 逐 entry 读取 tar 归档内容
 * 3. 将 ghostcoded 和 ghostcode-mcp 写入 targetDir
 * 4. 验证两个文件均已提取
 *
 * @param bundlePath 下载的 tar.gz 文件路径
 * @param targetDir 解压目标目录
 * @throws Error 解压失败或缺少必要文件时
 */
async function extractBundle(bundlePath: string, targetDir: string): Promise<void> {
  // 使用 Node.js 内置模块实现 tar.gz 解压
  // 由于 Node.js 没有内置 tar 解析器，使用流式处理
  // 实现一个简单的 tar 解析器
  await extractTarGz(bundlePath, targetDir);

  // 验证两个必要的二进制文件均已成功解压
  const daemonBin = join(targetDir, "ghostcoded");
  const mcpBin = join(targetDir, "ghostcode-mcp");

  if (!existsSync(daemonBin)) {
    throw new Error(`bundle 解压后缺少 ghostcoded 二进制: ${daemonBin}`);
  }
  if (!existsSync(mcpBin)) {
    throw new Error(`bundle 解压后缺少 ghostcode-mcp 二进制: ${mcpBin}`);
  }

  // 设置可执行权限（0o755: rwxr-xr-x）
  chmodSync(daemonBin, 0o755);
  chmodSync(mcpBin, 0o755);
}

/**
 * 解析并提取 tar.gz 文件
 * 使用纯 Node.js 内置模块实现，不依赖外部 tar 库
 *
 * tar 格式：每个文件由 512 字节的 header 块 + 数据块组成
 * 数据块按 512 字节对齐（使用 \0 填充）
 *
 * @param tarGzPath tar.gz 文件路径
 * @param destDir 解压目标目录
 */
async function extractTarGz(tarGzPath: string, destDir: string): Promise<void> {
  return new Promise((resolve, reject) => {
    // 读取整个 tar.gz 文件到内存（bundle 文件通常较小，<50MB）
    const gunzip = createGunzip();
    const inputStream = createReadStream(tarGzPath);

    const chunks: Buffer[] = [];

    gunzip.on("data", (chunk: Buffer) => {
      chunks.push(chunk);
    });

    gunzip.on("end", () => {
      try {
        // 合并所有解压后的 tar 数据
        const tarBuffer = Buffer.concat(chunks);
        // 解析 tar 归档并提取文件
        parseTarBuffer(tarBuffer, destDir);
        resolve();
      } catch (err) {
        reject(err);
      }
    });

    gunzip.on("error", reject);
    inputStream.on("error", reject);

    inputStream.pipe(gunzip);
  });
}

/**
 * 解析 tar 格式的 Buffer，提取文件到目标目录
 *
 * tar 格式说明：
 * - 每个 entry 以 512 字节 header 开头
 * - header 包含文件名（100字节）、文件大小（八进制字符串，12字节）等
 * - 数据跟随 header，按 512 字节对齐
 * - 归档以两个连续的全零 512 字节 header 结尾
 *
 * @param buffer 已解压的 tar 数据
 * @param destDir 目标目录
 */
function parseTarBuffer(buffer: Buffer, destDir: string): void {
  // tar header 大小为 512 字节
  const BLOCK_SIZE = 512;
  let offset = 0;

  while (offset < buffer.length) {
    // 检查是否到达 tar 归档结尾（两个连续的全零 block）
    if (offset + BLOCK_SIZE > buffer.length) {
      break;
    }

    const header = buffer.subarray(offset, offset + BLOCK_SIZE);

    // 检查是否为空 header（归档结尾标志）
    if (header.every((byte) => byte === 0)) {
      break;
    }

    // 解析文件名（header 偏移量 0，长度 100）
    const nameRaw = header.subarray(0, 100);
    const nullIdx = nameRaw.indexOf(0);
    const name = nameRaw.subarray(0, nullIdx >= 0 ? nullIdx : 100).toString("utf-8");

    // 解析文件大小（header 偏移量 124，长度 12，八进制字符串）
    const sizeRaw = header.subarray(124, 136);
    const sizeStr = sizeRaw.toString("utf-8").replace(/\0/g, "").trim();
    const fileSize = parseInt(sizeStr, 8);

    // 解析文件类型（header 偏移量 156，1 字节）
    // '0' 或 '\0' 表示普通文件，'5' 表示目录
    const typeflag = header[156];
    const isRegularFile = typeflag === 0x30 || typeflag === 0x00; // '0' 或 '\0'

    offset += BLOCK_SIZE;

    if (isRegularFile && fileSize > 0 && name) {
      // 提取文件数据（不含路径前缀，只取文件名部分）
      const fileName = basename(name);
      const fileData = buffer.subarray(offset, offset + fileSize);

      // 只提取需要的二进制文件
      if (fileName === "ghostcoded" || fileName === "ghostcode-mcp") {
        const destPath = join(destDir, fileName);
        writeFileSync(destPath, fileData);
      }
    }

    // 跳过数据块（按 512 字节对齐）
    if (fileSize > 0) {
      const alignedSize = Math.ceil(fileSize / BLOCK_SIZE) * BLOCK_SIZE;
      offset += alignedSize;
    }
  }
}

// ============================================
// Release 下载安装（新功能）
// ============================================

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
export async function installFromRelease(
  version: string,
  platform: string,
  arch: string,
  targetDir: string = join(GHOSTCODE_HOME, "bin")
): Promise<void> {
  // ============================================
  // 快速路径：已安装且版本匹配，直接返回
  // ============================================
  if (isInstalledInDir(version, targetDir)) {
    return;
  }

  // 确保目标目录存在
  mkdirSync(targetDir, { recursive: true });

  // 解析平台标识（验证平台合法性）
  const supportedPlatform = resolvePlatform(platform, arch);
  const bundleName = `ghostcode-${supportedPlatform}.tar.gz`;

  // ============================================
  // 生成下载 URL
  // ============================================
  const bundleUrl = buildReleaseAssetUrl(version, platform, arch);
  const checksumUrl = buildChecksumUrl(version);

  // ============================================
  // 下载 bundle 到临时目录
  // ============================================
  const tempDir = join(tmpdir(), `ghostcode-install-${Date.now()}`);
  mkdirSync(tempDir, { recursive: true });

  const bundleTempPath = join(tempDir, bundleName);
  const checksumTempPath = join(tempDir, "SHA256SUMS");

  try {
    // 下载 bundle（支持指数退避重试）
    await downloadWithRetry({
      url: bundleUrl,
      destPath: bundleTempPath,
    });

    // 下载 SHA256SUMS 文件
    await downloadWithRetry({
      url: checksumUrl,
      destPath: checksumTempPath,
    });

    // ============================================
    // 验证 checksum（checksum 不匹配时必须中止，禁止降级）
    // ============================================
    const checksumContent = readFileSync(checksumTempPath, "utf-8");
    const expectedHash = parseSha256Sums(checksumContent, bundleName);

    if (!expectedHash) {
      throw new Error(
        `SHA256SUMS 文件中未找到 ${bundleName} 的校验和。` +
        `版本 ${version} 的 Release 文件可能不完整。`
      );
    }

    const checksumMatches = await verifyChecksum(bundleTempPath, expectedHash);
    if (!checksumMatches) {
      // checksum 不匹配：立即中止，不覆盖任何现有文件，不做任何降级处理
      throw new Error(
        `Checksum 校验失败！bundle ${bundleName} 的 SHA256 不匹配。\n` +
        `期望: ${expectedHash}\n` +
        `文件可能已损坏或遭到篡改。安装已中止。`
      );
    }

    // ============================================
    // 解压 bundle 到目标目录
    // checksum 通过后才允许解压，确保安全性
    // ============================================
    await extractBundle(bundleTempPath, targetDir);

    // ============================================
    // 写入安装标记文件
    // ============================================
    writeInstalledMarkerToDir(version, supportedPlatform, targetDir);

  } finally {
    // 清理临时文件（无论成功还是失败）
    try {
      if (existsSync(bundleTempPath)) {
        unlinkSync(bundleTempPath);
      }
      if (existsSync(checksumTempPath)) {
        unlinkSync(checksumTempPath);
      }
    } catch {
      // 临时文件清理失败不影响主流程
    }
  }
}

// ============================================
// 主函数（本地复制模式，保留向后兼容）
// ============================================

/**
 * 执行 GhostCode Plugin 安装（本地复制模式）
 *
 * 业务逻辑：
 * 1. 读取当前 Plugin 版本
 * 2. 检查是否已安装且版本匹配（快速路径，跳过安装）
 * 3. 检测当前平台
 * 4. 定位 Plugin 包内对应平台的二进制文件
 * 5. 创建目标目录（~/.ghostcode/bin/）
 * 6. 复制二进制并设置可执行权限
 * 7. 写入安装标记文件
 *
 * @throws Error 当平台不受支持或二进制文件不存在时
 */
export async function installGhostcode(): Promise<void> {
  const currentVersion = readPluginVersion();

  // ============================================
  // 快速路径：已安装且版本匹配，直接返回
  // ============================================
  if (isAlreadyInstalled(currentVersion)) {
    return;
  }

  // ============================================
  // 检测平台
  // ============================================
  const platform = detectPlatform();
  const binaryName = platformToBinaryName(platform);

  // ============================================
  // 定位 Plugin 包内的预编译二进制
  // 二进制位于 Plugin 包根目录的 bin/ 子目录
  // import.meta.url 指向 dist/index.js，
  // 因此 bin/ 是 ../bin/（相对 dist/）
  // ============================================
  const pluginBinDir = join(dirname(new URL(import.meta.url).pathname), "..", "bin");
  const sourceBinaryPath = join(pluginBinDir, binaryName);

  if (!existsSync(sourceBinaryPath)) {
    throw new Error(
      `Plugin 包内缺少平台对应二进制: ${sourceBinaryPath}\n` +
      `请重新安装 GhostCode Plugin 或从 GitHub Release 手动下载。`
    );
  }

  // ============================================
  // 创建目标目录（~/.ghostcode/bin/）
  // ============================================
  const targetBinDir = dirname(DAEMON_BIN_PATH);
  mkdirSync(targetBinDir, { recursive: true });

  // ============================================
  // 复制二进制到安装目标路径
  // ============================================
  copyFileSync(sourceBinaryPath, DAEMON_BIN_PATH);

  // 设置可执行权限（0o755: rwxr-xr-x）
  chmodSync(DAEMON_BIN_PATH, 0o755);

  // ============================================
  // 写入安装标记文件
  // ============================================
  writeInstalledMarker(currentVersion, platform);
}
