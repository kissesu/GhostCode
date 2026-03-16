import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";
const MCP_BIN_NAME = "ghostcode-mcp";
const GHOSTCODE_SERVER_KEY = "ghostcode";
function resolveGhostcodeMcpPath(binDir) {
  const targetBinDir = binDir ?? join(homedir(), ".ghostcode", "bin");
  return join(targetBinDir, MCP_BIN_NAME);
}
function buildMcpJson(binDir) {
  const mcpPath = resolveGhostcodeMcpPath(binDir);
  return {
    mcpServers: {
      [GHOSTCODE_SERVER_KEY]: {
        command: mcpPath
      }
    }
  };
}
function mergeGhostcodeServerConfig(existing, binDir) {
  const mcpPath = resolveGhostcodeMcpPath(binDir);
  return {
    mcpServers: {
      // 保留所有原有 server 配置
      ...existing.mcpServers,
      // 覆盖/新增 ghostcode server（统一使用最新路径）
      [GHOSTCODE_SERVER_KEY]: {
        command: mcpPath
      }
    }
  };
}
async function writeMcpJson(targetPath, config) {
  const content = JSON.stringify(config, null, 2);
  writeFileSync(targetPath, content, "utf-8");
}
function readMcpJson(filePath) {
  if (!existsSync(filePath)) {
    return { mcpServers: {} };
  }
  try {
    const content = readFileSync(filePath, "utf-8");
    const parsed = JSON.parse(content);
    if (typeof parsed === "object" && parsed !== null && "mcpServers" in parsed && typeof parsed.mcpServers === "object") {
      return parsed;
    }
    return { mcpServers: {} };
  } catch {
    return { mcpServers: {} };
  }
}
export {
  buildMcpJson,
  mergeGhostcodeServerConfig,
  readMcpJson,
  writeMcpJson
};
//# sourceMappingURL=mcp-json.js.map