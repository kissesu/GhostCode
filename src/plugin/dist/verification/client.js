import { callDaemon } from "../ipc.js";
class VerificationError extends Error {
  /**
   * @param code - Daemon 返回的错误码，如 "NOT_FOUND"、"ALREADY_EXISTS"
   * @param message - 可读的错误描述
   */
  constructor(code, message) {
    super(message);
    this.code = code;
    this.name = "VerificationError";
  }
}
function extractRunState(resp, defaultMessage) {
  if (!resp.ok) {
    throw new VerificationError(
      resp.error?.code ?? "UNKNOWN",
      resp.error?.message ?? defaultMessage
    );
  }
  const result = resp.result;
  if (typeof result !== "object" || result === null || typeof result.run_id !== "string" || typeof result.status !== "string") {
    throw new VerificationError(
      "INVALID_RESPONSE",
      "Daemon \u8FD4\u56DE\u7684 RunState \u7ED3\u6784\u4E0D\u7B26\u5408\u9884\u671F"
    );
  }
  return result;
}
async function startVerification(groupId, runId) {
  const resp = await callDaemon("verification_start", {
    group_id: groupId,
    run_id: runId
  });
  return extractRunState(resp, "\u9A8C\u8BC1\u542F\u52A8\u5931\u8D25");
}
async function getVerificationStatus(groupId, runId) {
  const resp = await callDaemon("verification_status", {
    group_id: groupId,
    run_id: runId
  });
  return extractRunState(resp, "\u9A8C\u8BC1\u72B6\u6001\u67E5\u8BE2\u5931\u8D25");
}
async function cancelVerification(groupId, runId) {
  const resp = await callDaemon("verification_cancel", {
    group_id: groupId,
    run_id: runId
  });
  if (!resp.ok) {
    throw new VerificationError(
      resp.error?.code ?? "UNKNOWN",
      resp.error?.message ?? "\u9A8C\u8BC1\u53D6\u6D88\u5931\u8D25"
    );
  }
}
export {
  VerificationError,
  cancelVerification,
  getVerificationStatus,
  startVerification
};
//# sourceMappingURL=client.js.map