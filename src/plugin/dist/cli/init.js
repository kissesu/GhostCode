import { existsSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";
import { installFromRelease } from "../install.js";
import { mergeGhostcodeServerConfig, writeMcpJson, readMcpJson } from "./mcp-json.js";
const GHOSTCODE_HOME = join(homedir(), ".ghostcode");
const GHOSTCODE_BIN_DIR = join(GHOSTCODE_HOME, "bin");
const DAEMON_BIN_PATH = join(GHOSTCODE_BIN_DIR, "ghostcoded");
const MCP_BIN_PATH = join(GHOSTCODE_BIN_DIR, "ghostcode-mcp");
const DEFAULT_MCP_JSON_PATH = join(process.cwd(), ".mcp.json");
function ensureDirs() {
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
async function ensureBinaries(dryRun) {
  const daemonExists = existsSync(DAEMON_BIN_PATH);
  const mcpExists = existsSync(MCP_BIN_PATH);
  if (daemonExists && mcpExists) {
    return false;
  }
  if (dryRun) {
    return true;
  }
  const { createRequire } = await import("node:module");
  const require2 = createRequire(import.meta.url);
  const pkg = require2("../../package.json");
  const version = pkg.version ?? "0.1.0";
  await installFromRelease(version, process.platform, process.arch, GHOSTCODE_BIN_DIR);
  return true;
}
async function ensureMcpConfig(mcpJsonPath, dryRun) {
  const existing = readMcpJson(mcpJsonPath);
  const merged = mergeGhostcodeServerConfig(existing);
  if (dryRun) {
    return true;
  }
  await writeMcpJson(mcpJsonPath, merged);
  return true;
}
async function runInitCommand(options = {}) {
  const { dryRun = false, mcpJsonPath = DEFAULT_MCP_JSON_PATH } = options;
  try {
    const dirsCreated = ensureDirs();
    const binInstalled = await ensureBinaries(dryRun);
    const mcpJsonWritten = await ensureMcpConfig(mcpJsonPath, dryRun);
    return {
      success: true,
      dirsCreated,
      binInstalled,
      mcpJsonWritten
    };
  } catch (err) {
    const errorMsg = err instanceof Error ? err.message : String(err);
    return {
      success: false,
      dirsCreated: false,
      binInstalled: false,
      mcpJsonWritten: false,
      error: errorMsg
    };
  }
}
export {
  runInitCommand
};
//# sourceMappingURL=init.js.map