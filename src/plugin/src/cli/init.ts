/**
 * @file init.ts
 * @description ghostcode init 子命令
 *   初始化 GhostCode 运行环境：目录结构、二进制安装、MCP 配置
 * @author Atlas.oi
 * @date 2026-03-04
 */

import { existsSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";

import { installFromRelease } from "../install.js";
import { buildMcpJson, mergeGhostcodeServerConfig, writeMcpJson, readMcpJson } from "./mcp-json.js";

// ============================================
// 常量定义
// ============================================

/** GhostCode 主目录 */
const GHOSTCODE_HOME = join(homedir(), ".ghostcode");

/** 二进制安装目录 */
const GHOSTCODE_BIN_DIR = join(GHOSTCODE_HOME, "bin");

/** ghostcoded daemon 二进制路径 */
const DAEMON_BIN_PATH = join(GHOSTCODE_BIN_DIR, "ghostcoded");

/** ghostcode-mcp 二进制路径 */
const MCP_BIN_PATH = join(GHOSTCODE_BIN_DIR, "ghostcode-mcp");

/** 默认 .mcp.json 写入路径（当前工作目录） */
const DEFAULT_MCP_JSON_PATH = join(process.cwd(), ".mcp.json");

// ============================================
// 类型定义
// ============================================

/**
 * init 命令选项
 */
export interface InitOptions {
  /**
   * 测试模式：不执行真实的 fs 写入和网络下载，只返回会执行的操作描述
   * 主要用于单元测试中验证逻辑分支
   */
  dryRun?: boolean;
  /** .mcp.json 写入路径，默认为 process.cwd()/.mcp.json */
  mcpJsonPath?: string;
}

/**
 * init 命令执行结果
 */
export interface InitResult {
  /** 总体成功标志 */
  success: boolean;
  /** 是否创建了目录 */
  dirsCreated: boolean;
  /** 是否触发了二进制安装 */
  binInstalled: boolean;
  /** 是否写入/更新了 .mcp.json */
  mcpJsonWritten: boolean;
  /** 错误信息（失败时非空） */
  error?: string;
}

// ============================================
// 内部步骤函数
// ============================================

/**
 * 步骤 1：确保 GhostCode 目录结构存在
 *
 * 业务逻辑：
 * 1. 检查 ~/.ghostcode/ 是否存在，不存在则创建
 * 2. 检查 ~/.ghostcode/bin/ 是否存在，不存在则创建
 * 3. 使用 recursive: true 确保幂等（已存在不报错）
 *
 * @returns 是否实际创建了目录（true 表示有目录被创建，false 表示均已存在）
 */
function ensureDirs(): boolean {
  let created = false;

  if (!existsSync(GHOSTCODE_HOME)) {
    mkdirSync(GHOSTCODE_HOME, { recursive: true });
    created = true;
  }

  if (!existsSync(GHOSTCODE_BIN_DIR)) {
    mkdirSync(GHOSTCODE_BIN_DIR, { recursive: true });
    created = true;
  }

  return created;
}

/**
 * 步骤 2：确保必要的二进制文件已安装
 *
 * 业务逻辑：
 * 1. 检查 ghostcoded 和 ghostcode-mcp 是否均存在
 * 2. 如果任一不存在，调用 installFromRelease 下载安装
 * 3. 安装使用当前平台信息（process.platform, process.arch）
 *
 * @param dryRun 如果为 true，只检查不实际安装
 * @returns 是否触发了安装操作
 */
async function ensureBinaries(dryRun: boolean): Promise<boolean> {
  // 检查两个二进制是否均已就位
  const daemonExists = existsSync(DAEMON_BIN_PATH);
  const mcpExists = existsSync(MCP_BIN_PATH);

  if (daemonExists && mcpExists) {
    // 两个二进制均已存在，无需安装
    return false;
  }

  if (dryRun) {
    // dryRun 模式不执行真实安装
    return true;
  }

  // 读取 package.json 获取版本号
  // 使用动态 import 避免在顶层依赖 createRequire
  const { createRequire } = await import("node:module");
  const require = createRequire(import.meta.url);
  const pkg = require("../../package.json") as { version: string };
  const version = pkg.version ?? "0.1.0";

  // 触发 Release 安装
  await installFromRelease(version, process.platform, process.arch, GHOSTCODE_BIN_DIR);

  return true;
}

/**
 * 步骤 3：确保 .mcp.json 包含 ghostcode server 配置
 *
 * 业务逻辑：
 * 1. 读取目标路径的 .mcp.json（不存在时使用空配置）
 * 2. 合并 ghostcode server 配置（保留已有其他 server，更新 ghostcode）
 * 3. 将合并后的配置写回文件
 *
 * @param mcpJsonPath .mcp.json 目标路径
 * @param dryRun 如果为 true，只检查不实际写入
 * @returns 是否写入了 .mcp.json
 */
async function ensureMcpConfig(
  mcpJsonPath: string,
  dryRun: boolean
): Promise<boolean> {
  // 读取现有配置（不存在时返回空配置）
  const existing = readMcpJson(mcpJsonPath);

  // 合并 ghostcode server 配置
  const merged = mergeGhostcodeServerConfig(existing);

  if (dryRun) {
    return true;
  }

  // 写入合并后的配置
  await writeMcpJson(mcpJsonPath, merged);
  return true;
}

// ============================================
// 公开 API
// ============================================

/**
 * 执行 ghostcode init 命令
 *
 * 业务逻辑：
 * 1. ensureDirs() — 创建 ~/.ghostcode/ 和 ~/.ghostcode/bin/
 * 2. ensureBinaries() — 检查二进制是否存在，不存在则调用 installFromRelease
 * 3. ensureMcpConfig() — 生成/合并 .mcp.json（幂等操作）
 *
 * 幂等保证：
 * - 目录已存在时不报错（mkdirSync recursive）
 * - 二进制已存在时跳过安装（isInstalledInDir 检查）
 * - .mcp.json 已有 ghostcode 配置时执行更新而非重复添加
 *
 * @param options init 命令选项
 * @returns 执行结果对象
 */
export async function runInitCommand(
  options: InitOptions = {}
): Promise<InitResult> {
  const { dryRun = false, mcpJsonPath = DEFAULT_MCP_JSON_PATH } = options;

  try {
    // ============================================
    // 步骤 1：创建目录结构
    // ============================================
    const dirsCreated = ensureDirs();

    // ============================================
    // 步骤 2：确保二进制已安装
    // ============================================
    const binInstalled = await ensureBinaries(dryRun);

    // ============================================
    // 步骤 3：确保 .mcp.json 配置正确
    // ============================================
    const mcpJsonWritten = await ensureMcpConfig(mcpJsonPath, dryRun);

    return {
      success: true,
      dirsCreated,
      binInstalled,
      mcpJsonWritten,
    };
  } catch (err: unknown) {
    const errorMsg = err instanceof Error ? err.message : String(err);

    return {
      success: false,
      dirsCreated: false,
      binInstalled: false,
      mcpJsonWritten: false,
      error: errorMsg,
    };
  }
}
