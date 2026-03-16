import { join } from "node:path";
import { homedir } from "node:os";
import { createRequire } from "node:module";
import { installFromRelease } from "./install.js";
const GHOSTCODE_HOME = join(homedir(), ".ghostcode");
const GHOSTCODE_BIN_DIR = join(GHOSTCODE_HOME, "bin");
function isCIEnvironment() {
  return process.env["CI"] === "true" || process.env["GITHUB_ACTIONS"] === "true" || typeof process.env["JENKINS_URL"] === "string" && process.env["JENKINS_URL"].length > 0 || process.env["GITLAB_CI"] === "true";
}
function readPluginVersion() {
  try {
    const require2 = createRequire(import.meta.url);
    const pkg = require2("../package.json");
    return pkg.version || "unknown";
  } catch {
    return "unknown";
  }
}
function isPermissionError(err) {
  if (err instanceof Error) {
    return err.code === "EACCES";
  }
  return false;
}
async function runPostinstall() {
  if (isCIEnvironment()) {
    console.log("[GhostCode] CI \u73AF\u5883\uFF0C\u8DF3\u8FC7\u4E8C\u8FDB\u5236\u4E0B\u8F7D");
    return;
  }
  const version = readPluginVersion();
  try {
    await installFromRelease(version, process.platform, process.arch, GHOSTCODE_BIN_DIR);
    console.log("[GhostCode] \u5B89\u88C5\u5B8C\u6210");
    return;
  } catch (downloadErr) {
    if (isPermissionError(downloadErr)) {
      console.error(
        `[GhostCode] \u5B89\u88C5\u5931\u8D25\uFF1A\u6743\u9650\u4E0D\u8DB3
  \u8BF7\u68C0\u67E5\u76EE\u5F55\u6743\u9650\uFF1A${GHOSTCODE_BIN_DIR}
  \u4FEE\u590D\u5EFA\u8BAE\uFF1A
    chmod -R u+w ~/.ghostcode
    \u6216\u4F7F\u7528 sudo \u5B89\u88C5`
      );
      return;
    }
    const errMsg = downloadErr instanceof Error ? downloadErr.message : String(downloadErr);
    console.error(
      `[GhostCode] \u5B89\u88C5\u5931\u8D25\uFF1A\u65E0\u6CD5\u4ECE GitHub Release \u4E0B\u8F7D\u4E8C\u8FDB\u5236
  \u5931\u8D25\u539F\u56E0\uFF1A${errMsg}
  \u4FEE\u590D\u5EFA\u8BAE\uFF1A
    1. \u8FD0\u884C ghostcode doctor \u8BCA\u65AD\u95EE\u9898
    2. \u68C0\u67E5\u7F51\u7EDC\u8FDE\u63A5\u540E\u91CD\u65B0\u5B89\u88C5\uFF1Apnpm install
    3. \u624B\u52A8\u4ECE GitHub Release \u4E0B\u8F7D\u5E76\u653E\u7F6E\u5230 ${GHOSTCODE_BIN_DIR}
       https://github.com/kissesu/GhostCode/releases`
    );
  }
}
const isMainModule = process.argv[1] !== void 0 && import.meta.url.endsWith(process.argv[1] ?? "");
if (isMainModule) {
  runPostinstall().catch((err) => {
    const errMsg = err instanceof Error ? err.message : String(err);
    console.error(`[GhostCode] postinstall \u53D1\u751F\u672A\u9884\u671F\u9519\u8BEF\uFF1A${errMsg}`);
  });
}
export {
  isCIEnvironment,
  runPostinstall
};
//# sourceMappingURL=postinstall.js.map