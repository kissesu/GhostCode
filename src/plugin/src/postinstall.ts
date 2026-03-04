/**
 * @file postinstall.ts
 * @description npm/pnpm install 后自动触发的二进制安装脚本
 *              支持 CI 环境检测、GitHub Release 网络下载、离线包内二进制回退
 *
 * 业务流程：
 * 1. 检测 CI 环境 -> 跳过所有下载操作，仅输出提示
 * 2. 尝试 installFromRelease 下载最新版本
 * 3. 网络下载失败 -> 尝试回退到包内 bin/ 预编译二进制
 * 4. 回退也失败 -> 输出明确错误信息和修复建议
 * 5. 权限错误 -> 输出 chmod/sudo 相关提示
 *
 * @author Atlas.oi
 * @date 2026-03-04
 */

import {
  existsSync,
  readdirSync,
  copyFileSync,
  mkdirSync,
  chmodSync,
  unlinkSync,
} from "node:fs";
import { join, dirname } from "node:path";
import { homedir } from "node:os";
import { createRequire } from "node:module";

import { installFromRelease } from "./install.js";

// ============================================
// 常量定义
// ============================================

/** GhostCode 主目录 */
const GHOSTCODE_HOME = join(homedir(), ".ghostcode");

/** 二进制安装目标目录 */
const GHOSTCODE_BIN_DIR = join(GHOSTCODE_HOME, "bin");

// ============================================
// CI 环境检测
// ============================================

/**
 * 检测是否为 CI 环境
 *
 * 业务逻辑：
 * 检查常见 CI 平台的环境变量标识
 * - CI: GitHub Actions, CircleCI, Travis CI 等通用标识
 * - GITHUB_ACTIONS: GitHub Actions 专用
 * - JENKINS_URL: Jenkins CI
 * - GITLAB_CI: GitLab CI
 *
 * @returns true 表示当前运行在 CI 环境中
 */
export function isCIEnvironment(): boolean {
  return (
    process.env["CI"] === "true" ||
    process.env["GITHUB_ACTIONS"] === "true" ||
    typeof process.env["JENKINS_URL"] === "string" && process.env["JENKINS_URL"].length > 0 ||
    process.env["GITLAB_CI"] === "true"
  );
}

// ============================================
// 包内二进制回退逻辑
// ============================================

/**
 * 读取当前 Plugin 版本号（ESM 兼容方式）
 *
 * @returns 版本字符串，如 "0.1.0"
 */
function readPluginVersion(): string {
  try {
    const require = createRequire(import.meta.url);
    const pkg = require("../package.json") as { version: string };
    return pkg.version || "unknown";
  } catch {
    return "unknown";
  }
}

/**
 * 根据当前系统平台确定对应的预编译二进制文件名前缀
 *
 * 业务逻辑：
 * bin/ 目录下的文件名格式为 ghostcoded-{platform}
 * 需要匹配当前运行平台选取正确的二进制
 *
 * @returns 平台标识字符串，如 "darwin-arm64"，未匹配时返回 null
 */
function detectPlatformSuffix(): string | null {
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

  return null;
}

/**
 * 尝试从包内 bin/ 目录复制预编译二进制作为离线回退
 *
 * 业务逻辑：
 * 1. 定位包内 bin/ 目录（相对于编译后的 dist/ 目录）
 * 2. 检查是否存在对应平台的二进制文件
 * 3. 创建目标目录并复制二进制，设置可执行权限
 *
 * @param targetDir 二进制安装目标目录
 * @returns true 表示回退成功，false 表示包内无可用二进制
 */
export async function fallbackToLocalBin(targetDir: string): Promise<boolean> {
  // 定位包内 bin/ 目录：dist/postinstall.js -> ../bin/
  const pluginBinDir = join(dirname(new URL(import.meta.url).pathname), "..", "bin");

  // 检测当前平台对应的二进制后缀
  const platformSuffix = detectPlatformSuffix();
  if (!platformSuffix) {
    return false;
  }

  const daemonBinName = `ghostcoded-${platformSuffix}`;
  const sourceDaemonPath = join(pluginBinDir, daemonBinName);

  // 检查包内是否存在对应平台的 ghostcoded 二进制
  let sourceDaemonExists = false;
  try {
    sourceDaemonExists = existsSync(sourceDaemonPath);
  } catch {
    // existsSync 抛出异常（如权限不足），视为不存在
    return false;
  }

  if (!sourceDaemonExists) {
    // 尝试查找 bin/ 目录下是否有任何匹配的二进制
    try {
      const binFiles = readdirSync(pluginBinDir) as string[];
      const matchingBin = binFiles.find((f) => f.startsWith(`ghostcoded-${platformSuffix}`));
      if (!matchingBin) {
        return false;
      }
    } catch {
      return false;
    }
  }

  // 创建目标目录
  mkdirSync(targetDir, { recursive: true });

  // 复制 ghostcoded 二进制
  const targetDaemonPath = join(targetDir, "ghostcoded");
  copyFileSync(sourceDaemonPath, targetDaemonPath);
  chmodSync(targetDaemonPath, 0o755);

  // 同时复制 ghostcode-mcp 二进制（MCP 配置依赖此文件）
  // 如果包内不存在 mcp 二进制则回退失败，禁止产出不完整的安装
  const mcpBinName = `ghostcode-mcp-${platformSuffix}`;
  const sourceMcpPath = join(pluginBinDir, mcpBinName);
  let sourceMcpExists = false;
  try {
    sourceMcpExists = existsSync(sourceMcpPath);
  } catch {
    // 权限异常等情况
  }

  if (!sourceMcpExists) {
    // 包内缺少 mcp 二进制，清理已复制的 daemon，回退失败
    try {
      unlinkSync(targetDaemonPath);
    } catch {
      // 清理失败不影响逻辑
    }
    return false;
  }

  const targetMcpPath = join(targetDir, "ghostcode-mcp");
  copyFileSync(sourceMcpPath, targetMcpPath);
  chmodSync(targetMcpPath, 0o755);

  return true;
}

// ============================================
// 权限错误检测辅助函数
// ============================================

/**
 * 判断错误是否为权限错误（EACCES）
 *
 * @param err 错误对象
 * @returns true 表示权限不足
 */
function isPermissionError(err: unknown): boolean {
  if (err instanceof Error) {
    return (err as NodeJS.ErrnoException).code === "EACCES";
  }
  return false;
}

// ============================================
// postinstall 主入口
// ============================================

/**
 * postinstall 主入口函数
 *
 * 在 npm/pnpm install 完成后自动执行。
 * 输出保持简洁，不干扰包管理器的主控制台输出。
 *
 * 业务逻辑：
 * 1. CI 环境检测 -> 跳过下载
 * 2. installFromRelease 下载最新 bundle
 * 3. 下载失败 -> 回退到包内 bin/
 * 4. 双重失败 -> 输出错误和修复建议
 * 5. 权限错误 -> 输出 chmod/sudo 建议
 */
export async function runPostinstall(): Promise<void> {
  // ============================================
  // 步骤 1: CI 环境检测
  // CI 环境下完全跳过所有下载和复制操作
  // ============================================
  if (isCIEnvironment()) {
    console.log("[GhostCode] CI 环境，跳过二进制下载");
    return;
  }

  // ============================================
  // 步骤 2: 尝试从 GitHub Release 下载
  // ============================================
  const version = readPluginVersion();

  try {
    await installFromRelease(version, process.platform, process.arch, GHOSTCODE_BIN_DIR);
    console.log("[GhostCode] 安装完成");
    return;
  } catch (downloadErr: unknown) {
    // 权限错误需要特殊处理，输出修复建议
    if (isPermissionError(downloadErr)) {
      console.error(
        `[GhostCode] 安装失败：权限不足\n` +
        `  请检查目录权限：${GHOSTCODE_BIN_DIR}\n` +
        `  修复建议：\n` +
        `    chmod -R u+w ~/.ghostcode\n` +
        `    或使用 sudo 安装`
      );
      return;
    }

    const errMsg = downloadErr instanceof Error ? downloadErr.message : String(downloadErr);
    console.warn(`[GhostCode] 网络下载失败，尝试回退到包内二进制：${errMsg}`);
  }

  // ============================================
  // 步骤 3: 离线回退 - 尝试使用包内 bin/ 二进制
  // 仅在网络下载失败时触发，不是默认行为
  // ============================================
  try {
    const fallbackSuccess = await fallbackToLocalBin(GHOSTCODE_BIN_DIR);

    if (fallbackSuccess) {
      console.log("[GhostCode] 回退到包内二进制安装成功");
      return;
    }

    // 包内也没有可用的二进制
    console.error(
      `[GhostCode] 安装失败：网络下载失败且包内无可用二进制\n` +
      `  修复建议：\n` +
      `    1. 检查网络连接后重新安装：pnpm install\n` +
      `    2. 手动从 GitHub Release 下载并放置到 ${GHOSTCODE_BIN_DIR}\n` +
      `    3. 提交 Issue 报告问题`
    );

  } catch (fallbackErr: unknown) {
    // 回退也失败（如权限问题）
    if (isPermissionError(fallbackErr)) {
      console.error(
        `[GhostCode] 安装失败：权限不足（回退阶段）\n` +
        `  修复建议：\n` +
        `    chmod -R u+w ~/.ghostcode\n` +
        `    或使用 sudo 安装`
      );
      return;
    }

    const errMsg = fallbackErr instanceof Error ? fallbackErr.message : String(fallbackErr);
    console.error(
      `[GhostCode] 安装失败：所有安装方式均失败\n` +
      `  错误详情：${errMsg}\n` +
      `  修复建议：\n` +
      `    1. 检查网络连接后重新安装：pnpm install\n` +
      `    2. 手动从 GitHub Release 下载并放置到 ${GHOSTCODE_BIN_DIR}`
    );
  }
}

// ============================================
// 脚本直接执行入口
// 当作为 node dist/postinstall.js 执行时触发
// ============================================
// 使用 import.meta.url 检测是否为直接执行（ESM 兼容方式）
const isMainModule = process.argv[1] !== undefined &&
  import.meta.url.endsWith(process.argv[1] ?? "");

if (isMainModule) {
  runPostinstall().catch((err: unknown) => {
    const errMsg = err instanceof Error ? err.message : String(err);
    console.error(`[GhostCode] postinstall 发生未预期错误：${errMsg}`);
    // 不以非零退出码退出，避免阻断 npm install 流程
  });
}
