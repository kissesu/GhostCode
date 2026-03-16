import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";
import { createConnection } from "node:net";
const GHOSTCODE_HOME = join(homedir(), ".ghostcode");
const ADDR_JSON_PATH = join(GHOSTCODE_HOME, "daemon", "ghostcoded.addr.json");
const DAEMON_BIN_PATH = join(GHOSTCODE_HOME, "bin", "ghostcoded");
const DAEMON_START_TIMEOUT_MS = 5e3;
const ADDR_POLL_INTERVAL_MS = 100;
const PING_TIMEOUT_MS = 3e3;
const HEARTBEAT_INTERVAL_MS = 1e4;
const HEARTBEAT_MAX_FAILURES = 3;
const DAEMON_ENV_ALLOWLIST = [
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
  "NODE_ENV"
];
function createMinimalDaemonEnv() {
  const env = {};
  for (const key of DAEMON_ENV_ALLOWLIST) {
    if (process.env[key] !== void 0) {
      env[key] = process.env[key];
    }
  }
  return env;
}
function readAddrJson() {
  if (!existsSync(ADDR_JSON_PATH)) {
    return null;
  }
  try {
    const content = readFileSync(ADDR_JSON_PATH, "utf-8");
    const parsed = JSON.parse(content);
    if (parsed.v !== 1 || parsed.transport !== "unix" || !parsed.path) {
      return null;
    }
    return parsed;
  } catch {
    return null;
  }
}
function isProcessAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}
async function pingDaemon(addr) {
  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      socket.destroy();
      resolve(false);
    }, PING_TIMEOUT_MS);
    const socket = createConnection({ path: addr.path });
    let buffer = "";
    socket.on("connect", () => {
      const req = { v: 1, op: "ping", args: {} };
      socket.write(JSON.stringify(req) + "\n");
    });
    socket.on("data", (data) => {
      buffer += data.toString("utf-8");
      const newlineIdx = buffer.indexOf("\n");
      if (newlineIdx !== -1) {
        const line = buffer.slice(0, newlineIdx);
        clearTimeout(timer);
        socket.destroy();
        try {
          const resp = JSON.parse(line);
          const result = resp.result;
          resolve(resp.ok === true && result?.["pong"] === true);
        } catch {
          resolve(false);
        }
      }
    });
    socket.on("error", () => {
      clearTimeout(timer);
      resolve(false);
    });
    socket.on("close", () => {
      clearTimeout(timer);
      resolve(false);
    });
  });
}
async function waitForAddrJson() {
  const deadline = Date.now() + DAEMON_START_TIMEOUT_MS;
  while (Date.now() < deadline) {
    const addr = readAddrJson();
    if (addr !== null) {
      return addr;
    }
    await new Promise(
      (resolve) => setTimeout(resolve, ADDR_POLL_INTERVAL_MS)
    );
  }
  return null;
}
let _startingPromise = null;
let _cachedAddr = null;
async function ensureDaemon() {
  if (_cachedAddr !== null) {
    if (isProcessAlive(_cachedAddr.pid)) {
      return _cachedAddr;
    }
    _cachedAddr = null;
  }
  if (_startingPromise !== null) {
    return _startingPromise;
  }
  _startingPromise = _doEnsureDaemon().then((addr) => {
    _cachedAddr = addr;
    return addr;
  }).finally(() => {
    _startingPromise = null;
  });
  return _startingPromise;
}
async function _doEnsureDaemon() {
  const existingAddr = readAddrJson();
  if (existingAddr !== null) {
    const alive = isProcessAlive(existingAddr.pid);
    if (alive) {
      const pong2 = await pingDaemon(existingAddr);
      if (pong2) {
        return existingAddr;
      }
    }
  }
  await _spawnDaemon();
  const newAddr = await waitForAddrJson();
  if (newAddr === null) {
    throw new Error(
      `GhostCode Daemon \u542F\u52A8\u8D85\u65F6\uFF08${DAEMON_START_TIMEOUT_MS}ms\uFF09\uFF0C\u8BF7\u68C0\u67E5 ${DAEMON_BIN_PATH} \u662F\u5426\u5B58\u5728\u4E14\u53EF\u6267\u884C`
    );
  }
  const pong = await pingDaemon(newAddr);
  if (!pong) {
    throw new Error(
      "GhostCode Daemon \u542F\u52A8\u540E ping \u5931\u8D25\uFF0C\u53EF\u80FD Daemon \u521D\u59CB\u5316\u5F02\u5E38"
    );
  }
  return newAddr;
}
async function _spawnDaemon() {
  if (!existsSync(DAEMON_BIN_PATH)) {
    throw new Error(
      `GhostCode Daemon \u4E8C\u8FDB\u5236\u6587\u4EF6\u4E0D\u5B58\u5728: ${DAEMON_BIN_PATH}
\u8BF7\u5148\u5B89\u88C5 GhostCode \u6216\u68C0\u67E5\u5B89\u88C5\u8DEF\u5F84\u3002`
    );
  }
  return new Promise((resolve, reject) => {
    const child = spawn(DAEMON_BIN_PATH, [], {
      detached: true,
      stdio: "ignore",
      env: createMinimalDaemonEnv()
    });
    child.on("error", (err) => {
      reject(
        new Error(`spawn GhostCode Daemon \u5931\u8D25: ${err.message}`)
      );
    });
    child.on("spawn", () => {
      child.unref();
      resolve();
    });
  });
}
async function stopDaemon() {
  _cachedAddr = null;
  const addr = readAddrJson();
  if (addr === null) {
    return;
  }
  if (!isProcessAlive(addr.pid)) {
    return;
  }
  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      socket.destroy();
      resolve();
    }, PING_TIMEOUT_MS);
    const socket = createConnection({ path: addr.path });
    socket.on("connect", () => {
      const req = { v: 1, op: "shutdown", args: {} };
      socket.write(JSON.stringify(req) + "\n");
    });
    socket.on("data", (_data) => {
      clearTimeout(timer);
      socket.destroy();
      resolve();
    });
    socket.on("error", () => {
      clearTimeout(timer);
      resolve();
    });
    socket.on("close", () => {
      clearTimeout(timer);
      resolve();
    });
  });
}
function startHeartbeat(addr) {
  let failureCount = 0;
  let stopped = false;
  let currentAddr = addr;
  async function heartbeatTick() {
    if (stopped) {
      return;
    }
    try {
      const alive = await pingDaemon(currentAddr);
      if (alive) {
        failureCount = 0;
      } else {
        failureCount += 1;
        if (failureCount >= HEARTBEAT_MAX_FAILURES) {
          failureCount = 0;
          try {
            await stopDaemon();
          } catch {
          }
          const newAddr = await ensureDaemon();
          currentAddr = newAddr;
        }
      }
    } catch {
      failureCount += 1;
    }
  }
  const timer = setInterval(() => {
    void heartbeatTick();
  }, HEARTBEAT_INTERVAL_MS);
  return () => {
    stopped = true;
    clearInterval(timer);
  };
}
function getDaemonBinaryPath() {
  return DAEMON_BIN_PATH;
}
export {
  ensureDaemon,
  getDaemonBinaryPath,
  startHeartbeat,
  stopDaemon
};
//# sourceMappingURL=daemon.js.map