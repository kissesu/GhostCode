/**
 * @file postinstall.ts
 * @description npm/pnpm install 后自动触发的二进制安装脚本
 *              支持 CI 环境检测、GitHub Release 网络下载
 *
 * 业务流程：
 * 1. 检测 CI 环境 -> 跳过所有下载操作，仅输出提示
 * 2. 尝试 installFromRelease 下载最新版本
 * 3. 下载失败 -> 输出明确错误信息、失败原因和修复建议
 * 4. 权限错误 -> 输出 chmod/sudo 相关提示
 *
 * @author Atlas.oi
 * @date 2026-03-04
 */

import { join } from "node:path";
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
// 版本读取
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
 * 3. 下载失败 -> 直接报错，输出失败原因和修复建议（禁止降级回退）
 * 4. 权限错误 -> 输出 chmod/sudo 建议
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

    // ============================================
    // 步骤 3: 下载失败 -> 直接报错，输出明确的失败原因和修复建议
    // 禁止降级回退策略：问题应该暴露和修复，而不是用回退隐藏
    // ============================================
    const errMsg = downloadErr instanceof Error ? downloadErr.message : String(downloadErr);
    console.error(
      `[GhostCode] 安装失败：无法从 GitHub Release 下载二进制\n` +
      `  失败原因：${errMsg}\n` +
      `  修复建议：\n` +
      `    1. 运行 ghostcode doctor 诊断问题\n` +
      `    2. 检查网络连接后重新安装：pnpm install\n` +
      `    3. 手动从 GitHub Release 下载并放置到 ${GHOSTCODE_BIN_DIR}\n` +
      `       https://github.com/kissesu/GhostCode/releases`
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
