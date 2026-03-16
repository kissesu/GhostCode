import * as fs from "node:fs/promises";
import * as path from "node:path";
const STATE_FILE_RELATIVE_PATH = path.join(
  ".ghostcode",
  "state",
  "keywords.json"
);
const DEFAULT_STATE = {
  active: null,
  activatedAt: null,
  prompt: null
};
async function readKeywordState(workspaceRoot) {
  const statePath = path.join(workspaceRoot, STATE_FILE_RELATIVE_PATH);
  try {
    const content = await fs.readFile(statePath, "utf-8");
    return JSON.parse(content);
  } catch (err) {
    if (err.code === "ENOENT") {
      return { ...DEFAULT_STATE };
    }
    throw err;
  }
}
async function writeKeywordState(workspaceRoot, state) {
  const statePath = path.join(workspaceRoot, STATE_FILE_RELATIVE_PATH);
  const stateDir = path.dirname(statePath);
  await fs.mkdir(stateDir, { recursive: true });
  const content = JSON.stringify(state, null, 2);
  await fs.writeFile(statePath, content, "utf-8");
}
export {
  readKeywordState,
  writeKeywordState
};
//# sourceMappingURL=state.js.map