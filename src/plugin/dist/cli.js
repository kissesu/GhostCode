import { runInitCommand } from "./cli/init.js";
import { runDoctor, formatDoctorReport } from "./cli/doctor.js";
const VERSION = "0.1.0";
function printHelp() {
  console.log(`ghostcode v${VERSION}

\u7528\u6CD5\uFF1A
  ghostcode <\u547D\u4EE4> [\u9009\u9879]

\u547D\u4EE4\uFF1A
  init      \u521D\u59CB\u5316 GhostCode \u8FD0\u884C\u73AF\u5883\uFF08\u76EE\u5F55\u3001\u4E8C\u8FDB\u5236\u3001MCP \u914D\u7F6E\uFF09
  doctor    \u8BCA\u65AD GhostCode \u8FD0\u884C\u73AF\u5883\u5065\u5EB7\u72B6\u6001
  help      \u663E\u793A\u6B64\u5E2E\u52A9\u4FE1\u606F
  version   \u663E\u793A\u7248\u672C\u4FE1\u606F

\u793A\u4F8B\uFF1A
  ghostcode init              # \u521D\u59CB\u5316\u73AF\u5883
  ghostcode init --dry-run    # \u6A21\u62DF\u8FD0\u884C\uFF0C\u4E0D\u5B9E\u9645\u4FEE\u6539\u6587\u4EF6
  ghostcode doctor            # \u8FD0\u884C\u73AF\u5883\u8BCA\u65AD
`);
}
async function handleInit(args) {
  const dryRun = args.includes("--dry-run");
  let mcpJsonPath;
  const mcpJsonIdx = args.indexOf("--mcp-json");
  if (mcpJsonIdx >= 0) {
    mcpJsonPath = args[mcpJsonIdx + 1];
  }
  console.log("[GhostCode] \u5F00\u59CB\u521D\u59CB\u5316...");
  if (dryRun) {
    console.log("[GhostCode] \u6A21\u62DF\u8FD0\u884C\u6A21\u5F0F\uFF08--dry-run\uFF09\uFF0C\u4E0D\u4F1A\u4FEE\u6539\u6587\u4EF6");
  }
  const result = await runInitCommand({
    dryRun,
    ...mcpJsonPath !== void 0 ? { mcpJsonPath } : {}
  });
  if (!result.success) {
    console.error(`[GhostCode] \u521D\u59CB\u5316\u5931\u8D25\uFF1A${result.error ?? "\u672A\u77E5\u9519\u8BEF"}`);
    process.exit(1);
  }
  if (result.dirsCreated) {
    console.log("[GhostCode] \u76EE\u5F55\u7ED3\u6784\u5DF2\u521B\u5EFA\uFF1A~/.ghostcode/ \u548C ~/.ghostcode/bin/");
  } else {
    console.log("[GhostCode] \u76EE\u5F55\u7ED3\u6784\u5DF2\u5B58\u5728\uFF0C\u8DF3\u8FC7\u521B\u5EFA");
  }
  if (result.binInstalled) {
    console.log("[GhostCode] \u4E8C\u8FDB\u5236\u6587\u4EF6\u5B89\u88C5\u5B8C\u6210");
  } else {
    console.log("[GhostCode] \u4E8C\u8FDB\u5236\u6587\u4EF6\u5DF2\u5B58\u5728\uFF0C\u8DF3\u8FC7\u5B89\u88C5");
  }
  if (result.mcpJsonWritten) {
    console.log("[GhostCode] .mcp.json \u914D\u7F6E\u5DF2\u66F4\u65B0");
  }
  console.log("[GhostCode] \u521D\u59CB\u5316\u5B8C\u6210\uFF01");
}
async function handleDoctor() {
  const report = await runDoctor();
  console.log(formatDoctorReport(report));
  if (report.overallStatus === "FAIL") {
    process.exit(1);
  }
}
async function main(argv = process.argv) {
  const args = argv.slice(2);
  const command = args[0];
  if (!command || command === "help" || command === "--help" || command === "-h") {
    printHelp();
    return;
  }
  if (command === "version" || command === "--version" || command === "-v") {
    console.log(`ghostcode v${VERSION}`);
    return;
  }
  if (command === "init") {
    await handleInit(args.slice(1));
    return;
  }
  if (command === "doctor") {
    await handleDoctor();
    return;
  }
  console.error(`[GhostCode] \u672A\u77E5\u547D\u4EE4\uFF1A${command}`);
  printHelp();
  process.exit(1);
}
const isMainModule = process.argv[1] !== void 0 && (process.argv[1].endsWith("cli.js") || process.argv[1].endsWith("cli.ts"));
if (isMainModule) {
  main().catch((err) => {
    const errMsg = err instanceof Error ? err.message : String(err);
    console.error(`[GhostCode] CLI \u53D1\u751F\u672A\u9884\u671F\u9519\u8BEF\uFF1A${errMsg}`);
    process.exit(1);
  });
}
export {
  main
};
//# sourceMappingURL=cli.js.map