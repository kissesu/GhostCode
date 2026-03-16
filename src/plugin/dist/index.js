import { installGhostcode } from "./install.js";
await installGhostcode();
import { ensureDaemon, stopDaemon, startHeartbeat, getDaemonBinaryPath } from "./daemon.js";
import {
  callDaemon,
  createConnection,
  resetClient,
  IpcTimeoutError,
  IpcConnectionError,
  IpcProtocolError
} from "./ipc.js";
import { registerHook, getHooks, clearHooks, initializeHooks, preToolUseHandler, stopHandler } from "./hooks/index.js";
const VERSION = "0.1.0";
const PLUGIN_NAME = "ghostcode";
export {
  IpcConnectionError,
  IpcProtocolError,
  IpcTimeoutError,
  PLUGIN_NAME,
  VERSION,
  callDaemon,
  clearHooks,
  createConnection,
  ensureDaemon,
  getDaemonBinaryPath,
  getHooks,
  initializeHooks,
  preToolUseHandler,
  registerHook,
  resetClient,
  startHeartbeat,
  stopDaemon,
  stopHandler
};
//# sourceMappingURL=index.js.map