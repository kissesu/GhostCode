/**
 * @file mcp-json.test.ts
 * @description .mcp.json 配置生成器的单元测试
 *   TDD Red 阶段：先写测试，验证测试失败，再实现
 * @author Atlas.oi
 * @date 2026-03-04
 */

import { describe, it, expect } from "vitest";
import { buildMcpJson, mergeGhostcodeServerConfig } from "../cli/mcp-json.js";
import { homedir } from "node:os";
import { join } from "node:path";

// ============================================
// 类型定义（测试用，与实现保持一致）
// ============================================

// MCP server 配置结构
interface McpServerConfig {
  command: string;
  args?: string[];
  env?: Record<string, string>;
}

// .mcp.json 顶层结构
interface McpConfig {
  mcpServers: Record<string, McpServerConfig>;
}

// ============================================
// 测试套件
// ============================================

describe("buildMcpJson", () => {
  it("生成 .mcp.json 指向 ghostcode-mcp 二进制", () => {
    // 调用 buildMcpJson，使用默认 binDir
    const config = buildMcpJson();

    // 验证顶层结构
    expect(config).toHaveProperty("mcpServers");
    expect(config.mcpServers).toHaveProperty("ghostcode");

    // 验证 command 指向 ~/.ghostcode/bin/ghostcode-mcp
    const expectedBinPath = join(homedir(), ".ghostcode", "bin", "ghostcode-mcp");
    const ghostcodeServer = config.mcpServers["ghostcode"];
    expect(ghostcodeServer).toBeDefined();
    expect(ghostcodeServer!.command).toBe(expectedBinPath);
  });

  it("自定义 binDir 时 command 指向该目录下的 ghostcode-mcp", () => {
    const customBinDir = "/custom/bin/dir";
    const config = buildMcpJson(customBinDir);

    const ghostcodeServer = config.mcpServers["ghostcode"];
    expect(ghostcodeServer).toBeDefined();
    expect(ghostcodeServer!.command).toBe(join(customBinDir, "ghostcode-mcp"));
  });
});

describe("mergeGhostcodeServerConfig", () => {
  it("已有非 ghostcode mcpServers 配置时合并保留原有 server", () => {
    // 模拟已有 .mcp.json 包含其他 server 配置
    const existing: McpConfig = {
      mcpServers: {
        "other-server": {
          command: "/usr/local/bin/other-mcp",
          args: ["--port", "8080"],
        },
      },
    };

    const merged = mergeGhostcodeServerConfig(existing);

    // 验证原有 server 保留
    expect(merged.mcpServers).toHaveProperty("other-server");
    expect(merged.mcpServers["other-server"]!.command).toBe("/usr/local/bin/other-mcp");

    // 验证 ghostcode server 被添加
    expect(merged.mcpServers).toHaveProperty("ghostcode");
    const expectedBinPath = join(homedir(), ".ghostcode", "bin", "ghostcode-mcp");
    expect(merged.mcpServers["ghostcode"]!.command).toBe(expectedBinPath);
  });

  it("已有 ghostcode 配置时更新而非重复", () => {
    // 模拟已有 .mcp.json 包含旧版 ghostcode 配置（旧路径）
    const existing: McpConfig = {
      mcpServers: {
        ghostcode: {
          command: "/old/path/ghostcode-mcp",
        },
      },
    };

    const merged = mergeGhostcodeServerConfig(existing);

    // 验证 ghostcode 配置被更新，不会出现两个
    const ghostcodeKeys = Object.keys(merged.mcpServers).filter((k) => k === "ghostcode");
    expect(ghostcodeKeys).toHaveLength(1);

    // 验证 command 已更新为新路径
    const expectedBinPath = join(homedir(), ".ghostcode", "bin", "ghostcode-mcp");
    expect(merged.mcpServers["ghostcode"]!.command).toBe(expectedBinPath);
  });

  it("空 mcpServers 时只添加 ghostcode", () => {
    const existing: McpConfig = {
      mcpServers: {},
    };

    const merged = mergeGhostcodeServerConfig(existing);

    const keys = Object.keys(merged.mcpServers);
    expect(keys).toHaveLength(1);
    expect(keys[0]).toBe("ghostcode");
  });
});
