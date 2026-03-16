import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";
const GHOSTCODE_HOME = join(homedir(), ".ghostcode");
const WEB_BIN_PATH = join(GHOSTCODE_HOME, "bin", "ghostcode-web");
const WEB_HOST = "127.0.0.1";
const WEB_PORT = 7070;
const HEALTH_CHECK_TIMEOUT_MS = 2e3;
const WEB_START_TIMEOUT_MS = 8e3;
const WEB_POLL_INTERVAL_MS = 300;
const WEB_ENV_ALLOWLIST = [
  "PATH",
  "HOME",
  "USER",
  "USERNAME",
  "LOGNAME",
  "LANG",
  "LC_ALL",
  "LC_CTYPE",
  "TERM",
  "TMPDIR",
  "TMP",
  "TEMP",
  "XDG_RUNTIME_DIR",
  "XDG_DATA_HOME",
  "XDG_CONFIG_HOME",
  "SHELL",
  "NODE_ENV",
  "RUST_LOG"
];
let _startingPromise = null;
let _webReady = false;
function createMinimalWebEnv() {
  const env = {};
  for (const key of WEB_ENV_ALLOWLIST) {
    if (process.env[key] !== void 0) {
      env[key] = process.env[key];
    }
  }
  return env;
}
async function isWebRunning() {
  try {
    const response = await fetch(`http://${WEB_HOST}:${WEB_PORT}/health`, {
      signal: AbortSignal.timeout(HEALTH_CHECK_TIMEOUT_MS)
    });
    return response.ok;
  } catch {
    return false;
  }
}
async function waitForWebReady() {
  const deadline = Date.now() + WEB_START_TIMEOUT_MS;
  while (Date.now() < deadline) {
    if (await isWebRunning()) {
      return true;
    }
    await new Promise(
      (resolve) => setTimeout(resolve, WEB_POLL_INTERVAL_MS)
    );
  }
  return false;
}
async function spawnWeb() {
  if (!existsSync(WEB_BIN_PATH)) {
    throw new Error(
      `ghostcode-web \u4E8C\u8FDB\u5236\u6587\u4EF6\u4E0D\u5B58\u5728: ${WEB_BIN_PATH}
\u8BF7\u5148\u7F16\u8BD1: cargo build --release -p ghostcode-web`
    );
  }
  return new Promise((resolve, reject) => {
    const child = spawn(WEB_BIN_PATH, [], {
      detached: true,
      stdio: "ignore",
      env: createMinimalWebEnv()
    });
    child.on("error", (err) => {
      reject(new Error(`spawn ghostcode-web \u5931\u8D25: ${err.message}`));
    });
    child.on("spawn", () => {
      child.unref();
      resolve();
    });
  });
}
async function ensureWeb() {
  const dashboardUrl = `http://${WEB_HOST}:${WEB_PORT}`;
  if (_webReady) {
    return dashboardUrl;
  }
  if (_startingPromise !== null) {
    await _startingPromise;
    return dashboardUrl;
  }
  _startingPromise = _doEnsureWeb().then(() => {
    _webReady = true;
  }).finally(() => {
    _startingPromise = null;
  });
  await _startingPromise;
  return dashboardUrl;
}
async function _doEnsureWeb() {
  if (await isWebRunning()) {
    return;
  }
  await spawnWeb();
  const ready = await waitForWebReady();
  if (!ready) {
    throw new Error(
      `ghostcode-web \u542F\u52A8\u8D85\u65F6\uFF08${WEB_START_TIMEOUT_MS}ms\uFF09\uFF0C\u8BF7\u68C0\u67E5 ${WEB_BIN_PATH} \u662F\u5426\u5B58\u5728\u4E14\u53EF\u6267\u884C`
    );
  }
}
function getWebUrl() {
  return `http://${WEB_HOST}:${WEB_PORT}`;
}
function getWebPort() {
  return WEB_PORT;
}
function resetWebState() {
  _webReady = false;
  _startingPromise = null;
}
export {
  ensureWeb,
  getWebPort,
  getWebUrl,
  resetWebState
};
//# sourceMappingURL=web.js.map