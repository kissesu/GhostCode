/**
 * @file mcp-json.ts
 * @description .mcp.json 配置生成器
 *   生成和合并 MCP server 配置，确保 ghostcode-mcp 正确注册到 Claude Code
 * @author Atlas.oi
 * @date 2026-03-04
 */

import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";

// ============================================
// 类型定义
// ============================================

/**
 * MCP server 单条配置
 */
export interface McpServerConfig {
  /** server 可执行文件路径 */
  command: string;
  /** 传给 server 的参数列表 */
  args?: string[];
  /** server 的环境变量 */
  env?: Record<string, string>;
}

/**
 * .mcp.json 顶层结构
 */
export interface McpConfig {
  mcpServers: Record<string, McpServerConfig>;
}

// ============================================
// 常量
// ============================================

/** ghostcode-mcp 二进制名称 */
const MCP_BIN_NAME = "ghostcode-mcp";

/** .mcp.json 中 ghostcode server 的注册名 */
const GHOSTCODE_SERVER_KEY = "ghostcode";

// ============================================
// 工具函数
// ============================================

/**
 * 构建 ghostcode-mcp 二进制的默认安装路径
 *
 * @param binDir 可选自定义 binDir，默认为 ~/.ghostcode/bin
 * @returns ghostcode-mcp 的完整路径
 */
function resolveGhostcodeMcpPath(binDir?: string): string {
  const targetBinDir = binDir ?? join(homedir(), ".ghostcode", "bin");
  return join(targetBinDir, MCP_BIN_NAME);
}

// ============================================
// 公开 API
// ============================================

/**
 * 构建全新的 .mcp.json 配置对象
 *
 * 业务逻辑：
 * 1. 解析 ghostcode-mcp 二进制路径（默认 ~/.ghostcode/bin/ghostcode-mcp）
 * 2. 返回包含 ghostcode server 条目的 McpConfig 对象
 *
 * @param binDir 可选自定义 binDir，默认为 ~/.ghostcode/bin
 * @returns 新的 McpConfig 对象
 */
export function buildMcpJson(binDir?: string): McpConfig {
  const mcpPath = resolveGhostcodeMcpPath(binDir);

  return {
    mcpServers: {
      [GHOSTCODE_SERVER_KEY]: {
        command: mcpPath,
      },
    },
  };
}

/**
 * 将 ghostcode server 配置合并到已有的 McpConfig 对象中
 *
 * 业务逻辑：
 * 1. 保留 existing.mcpServers 中所有非 ghostcode 的 server 配置
 * 2. 如果已有 ghostcode 配置，更新 command 为当前路径（不重复添加）
 * 3. 如果不存在 ghostcode 配置，新增该条目
 *
 * 注意：此函数返回新对象，不修改传入的 existing 参数（不可变更新）
 *
 * @param existing 现有的 McpConfig 对象
 * @param binDir 可选自定义 binDir，默认为 ~/.ghostcode/bin
 * @returns 合并后的新 McpConfig 对象
 */
export function mergeGhostcodeServerConfig(
  existing: McpConfig,
  binDir?: string
): McpConfig {
  const mcpPath = resolveGhostcodeMcpPath(binDir);

  return {
    mcpServers: {
      // 保留所有原有 server 配置
      ...existing.mcpServers,
      // 覆盖/新增 ghostcode server（统一使用最新路径）
      [GHOSTCODE_SERVER_KEY]: {
        command: mcpPath,
      },
    },
  };
}

/**
 * 将 McpConfig 对象写入到指定的 .mcp.json 文件路径
 *
 * 业务逻辑：
 * 1. 将 config 序列化为格式化 JSON（2 空格缩进）
 * 2. 以 UTF-8 编码写入目标路径
 *
 * @param targetPath 目标文件路径（通常为项目根目录的 .mcp.json）
 * @param config 要写入的 McpConfig 对象
 */
export async function writeMcpJson(
  targetPath: string,
  config: McpConfig
): Promise<void> {
  const content = JSON.stringify(config, null, 2);
  writeFileSync(targetPath, content, "utf-8");
}

/**
 * 从文件路径读取并解析 .mcp.json
 *
 * 业务逻辑：
 * 1. 如果文件不存在，返回空的 McpConfig
 * 2. 读取文件内容并 JSON.parse
 * 3. 如果内容格式不合法（缺少 mcpServers），返回空配置
 *
 * @param filePath .mcp.json 文件路径
 * @returns 解析后的 McpConfig，文件不存在或格式错误时返回空配置
 */
export function readMcpJson(filePath: string): McpConfig {
  if (!existsSync(filePath)) {
    return { mcpServers: {} };
  }

  try {
    const content = readFileSync(filePath, "utf-8");
    const parsed = JSON.parse(content) as unknown;

    // 验证顶层结构是否合法
    if (
      typeof parsed === "object" &&
      parsed !== null &&
      "mcpServers" in parsed &&
      typeof (parsed as McpConfig).mcpServers === "object"
    ) {
      return parsed as McpConfig;
    }

    // 格式不合法，返回空配置
    return { mcpServers: {} };
  } catch {
    // JSON 解析失败，返回空配置
    return { mcpServers: {} };
  }
}
