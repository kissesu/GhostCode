import { ensureDaemon, stopDaemon, startHeartbeat } from "../daemon.js";
import { ensureWeb } from "../web.js";
import { SessionLeaseManager } from "../session-lease.js";
import { join } from "node:path";
import { homedir } from "node:os";
import { registerHook } from "./registry.js";
import { detectMagicKeywords, resolveKeywordPriority } from "../keywords/index.js";
import { writeKeywordState } from "../keywords/state.js";
import { appendSessionContent } from "../learner/manager.js";
const WORKSPACE_ROOT = process.cwd();
let daemonPromise = null;
let stopHeartbeat = null;
const leaseManager = new SessionLeaseManager(
  join(homedir(), ".ghostcode", "daemon", "sessions.json")
);
let currentLeaseId = null;
async function preToolUseHandler(_event) {
  if (daemonPromise !== null) {
    return;
  }
  daemonPromise = ensureDaemon();
  let addr;
  try {
    addr = await daemonPromise;
  } catch (err) {
    console.error("[GhostCode] Daemon \u542F\u52A8\u5931\u8D25\uFF0C\u5DE5\u5177\u8C03\u7528\u5C06\u7EE7\u7EED\u4F46\u65E0\u534F\u4F5C\u529F\u80FD:", err);
    daemonPromise = null;
    return;
  }
  process.env["GHOSTCODE_SOCKET_PATH"] = addr.path;
  try {
    stopHeartbeat = startHeartbeat(addr);
  } catch {
    console.error("[GhostCode] \u5FC3\u8DF3\u542F\u52A8\u5931\u8D25\uFF0CDaemon \u4ECD\u53EF\u6B63\u5E38\u4F7F\u7528");
  }
  try {
    await ensureWeb();
  } catch {
    console.error("[GhostCode] Dashboard \u81EA\u52A8\u542F\u52A8\u5931\u8D25\uFF0C\u53EF\u624B\u52A8\u8FD0\u884C ghostcode-web");
  }
  if (currentLeaseId === null) {
    try {
      const lease = leaseManager.acquireLease();
      currentLeaseId = lease.leaseId;
    } catch {
      console.error("[GhostCode] Session lease \u83B7\u53D6\u5931\u8D25\uFF0C\u505C\u6B62\u65F6\u5C06\u5B89\u5168\u964D\u7EA7");
    }
  }
}
async function stopHandler(_event) {
  if (stopHeartbeat !== null) {
    stopHeartbeat();
    stopHeartbeat = null;
  }
  try {
    const { onSessionEnd } = await import("../learner/index.js");
    await onSessionEnd();
  } catch {
    console.error("[GhostCode] Skill Learning \u5206\u6790\u5931\u8D25\uFF0C\u7EE7\u7EED\u6267\u884C Stop \u6D41\u7A0B");
  }
  let shouldShutdown = false;
  if (currentLeaseId !== null) {
    try {
      const result = leaseManager.releaseLease(currentLeaseId);
      shouldShutdown = result.isLast;
    } catch {
      console.error("[GhostCode] Lease \u91CA\u653E\u5931\u8D25\uFF0C\u4FDD\u5B88\u4FDD\u7559 Daemon \u8FD0\u884C");
    }
    currentLeaseId = null;
  } else {
    try {
      const refcount = leaseManager.getRefcount();
      shouldShutdown = refcount === 0;
    } catch {
      console.error("[GhostCode] \u65E0\u6CD5\u8BFB\u53D6 refcount\uFF0C\u4FDD\u5B88\u4FDD\u7559 Daemon \u8FD0\u884C");
    }
  }
  if (shouldShutdown) {
    await stopDaemon();
    daemonPromise = null;
  }
}
const KEYWORD_CONTEXT_MAP = {
  ralph: "[GhostCode] Ralph \u9A8C\u8BC1\u6A21\u5F0F\u5DF2\u6FC0\u6D3B - \u4EE3\u7801\u53D8\u66F4\u5C06\u7ECF\u8FC7 7 \u9879\u81EA\u52A8\u9A8C\u8BC1",
  autopilot: "[GhostCode] Autopilot \u6A21\u5F0F\u5DF2\u6FC0\u6D3B - \u5168\u81EA\u52A8\u6267\u884C\u6A21\u5F0F",
  team: "[GhostCode] Team \u6A21\u5F0F\u5DF2\u6FC0\u6D3B - \u591A Agent \u534F\u4F5C\u6A21\u5F0F",
  ultrawork: "[GhostCode] UltraWork \u6A21\u5F0F\u5DF2\u6FC0\u6D3B - \u6781\u81F4\u5DE5\u4F5C\u6A21\u5F0F"
};
async function userPromptSubmitHandler(event) {
  const eventObj = typeof event === "object" && event !== null ? event : null;
  const prompt = eventObj !== null ? String(
    (typeof eventObj["event"] === "object" && eventObj["event"] !== null ? eventObj["event"]["prompt"] : void 0) ?? eventObj["prompt"] ?? ""
  ) : "";
  if (!prompt) {
    return void 0;
  }
  appendSessionContent(prompt);
  const matches = detectMagicKeywords(prompt);
  const topMatch = resolveKeywordPriority(matches);
  if (topMatch === null) {
    return void 0;
  }
  if (topMatch.type === "cancel") {
    const clearState = {
      active: null,
      activatedAt: null,
      prompt: null
    };
    try {
      await writeKeywordState(WORKSPACE_ROOT, clearState);
    } catch {
    }
    return { additionalContext: "[GhostCode] \u6A21\u5F0F\u5DF2\u53D6\u6D88" };
  }
  const newState = {
    active: topMatch.type,
    activatedAt: (/* @__PURE__ */ new Date()).toISOString(),
    prompt
  };
  try {
    await writeKeywordState(WORKSPACE_ROOT, newState);
  } catch {
  }
  const contextMessage = KEYWORD_CONTEXT_MAP[topMatch.type] ?? `[GhostCode] ${topMatch.type} \u6A21\u5F0F\u5DF2\u6FC0\u6D3B`;
  return { additionalContext: contextMessage };
}
function initializeHooks() {
  registerHook("PreToolUse", preToolUseHandler);
  registerHook("Stop", stopHandler);
  registerHook("UserPromptSubmit", userPromptSubmitHandler);
}
export {
  initializeHooks,
  preToolUseHandler,
  stopHandler,
  userPromptSubmitHandler
};
//# sourceMappingURL=handlers.js.map