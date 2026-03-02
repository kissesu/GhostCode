/**
 * @file install.ts
 * @description GhostCode Plugin 首次运行安装逻辑
 *              检测当前平台，将对应预编译二进制复制到 ~/.ghostcode/bin/ghostcoded
 *              通过 ~/.ghostcode/.installed 标记文件避免重复安装
 * @author Atlas.oi
 * @date 2026-03-01
 */

import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync, chmodSync } from "node:fs";
import { dirname, join } from "node:path";
import { homedir } from "node:os";
import { createRequire } from "node:module";

// ============================================
// 常量定义
// ============================================

/** GhostCode 主目录 */
const GHOSTCODE_HOME = join(homedir(), ".ghostcode");

/** Daemon 二进制安装目标路径 */
const DAEMON_BIN_PATH = join(GHOSTCODE_HOME, "bin", "ghostcoded");

/** 安装标记文件路径（记录已安装的版本） */
const INSTALLED_MARKER_PATH = join(GHOSTCODE_HOME, ".installed");

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
// 辅助函数
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
 * 检查是否已安装且版本匹配
 *
 * 业务逻辑：
 * 1. 读取标记文件
 * 2. 解析 JSON，比对版本号
 * 3. 验证二进制文件是否仍然存在
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

/**
 * 写入安装标记文件
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

// ============================================
// 主函数
// ============================================

/**
 * 执行 GhostCode Plugin 安装
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
