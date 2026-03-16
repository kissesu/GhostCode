import { readdir, readFile } from "node:fs/promises";
import { join, extname } from "node:path";
import { createHash } from "node:crypto";
async function loadSkillsFromDir(dir) {
  let files;
  try {
    files = await readdir(dir);
  } catch {
    return [];
  }
  const skills = [];
  for (const filename of files) {
    if (extname(filename) !== ".md") continue;
    const filepath = join(dir, filename);
    try {
      const content = await readFile(filepath, "utf-8");
      const skill = parseSkillFile(filepath, filename, content);
      if (skill) skills.push(skill);
    } catch {
    }
  }
  return skills;
}
function parseSkillFile(filepath, filename, content) {
  const match = content.match(/^---\n([\s\S]*?)\n---\n?([\s\S]*)$/);
  if (!match) return null;
  const yamlStr = match[1] ?? "";
  const body = match[2] ?? "";
  const metadata = parseYaml(yamlStr);
  if (!metadata.id || !metadata.name) return null;
  const contentHash = createHash("sha256").update(content).digest("hex").slice(0, 16);
  return {
    path: filepath,
    relativePath: filename,
    scope: filepath.includes(".claude/skills") ? "project" : "user",
    metadata: {
      id: metadata.id,
      name: metadata.name,
      description: metadata.description ?? "",
      triggers: parseTriggers(metadata.triggers),
      createdAt: metadata.createdAt ?? (/* @__PURE__ */ new Date()).toISOString(),
      source: metadata.source ?? "manual",
      quality: Number(metadata.quality ?? 0),
      usageCount: Number(metadata.usageCount ?? 0),
      tags: parseTriggers(metadata.tags)
    },
    content: body.trim(),
    // body 已在上方 narrowing 处理，此处不会是 undefined
    contentHash,
    priority: filepath.includes(".claude/skills") ? 10 : 5
  };
}
function parseYaml(yaml) {
  const result = {};
  for (const line of yaml.split("\n")) {
    const idx = line.indexOf(":");
    if (idx === -1) continue;
    const key = line.slice(0, idx).trim();
    const value = line.slice(idx + 1).trim().replace(/^["']|["']$/g, "");
    if (key) result[key] = value;
  }
  return result;
}
function parseTriggers(value) {
  if (!value) return [];
  return value.split(",").map((s) => s.trim()).filter(Boolean);
}
export {
  loadSkillsFromDir
};
//# sourceMappingURL=loader.js.map