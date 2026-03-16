import { callDaemon } from "../ipc.js";
import { detectPatterns } from "./detector.js";
let sessionContent = "";
function appendSessionContent(content) {
  sessionContent += `
${content}`;
}
async function onSessionEnd() {
  const content = sessionContent;
  sessionContent = "";
  if (content.length < 100) return;
  const patterns = detectPatterns(content);
  for (const pattern of patterns) {
    if (pattern.confidence < 70) continue;
    try {
      await callDaemon("skill_learn_fragment", {
        problem: pattern.problem,
        solution: pattern.solution,
        confidence: pattern.confidence,
        context: content.slice(0, 500),
        suggested_triggers: pattern.suggestedTriggers,
        suggested_tags: pattern.suggestedTags
      });
    } catch {
    }
  }
}
async function listCandidates() {
  try {
    const resp = await callDaemon("skill_list");
    if (resp.ok && Array.isArray(resp.result)) {
      return resp.result;
    }
  } catch {
  }
  return [];
}
export {
  appendSessionContent,
  listCandidates,
  onSessionEnd
};
//# sourceMappingURL=manager.js.map