/**
 * @file loader.ts
 * @description Skill 文件加载器
 *              从指定目录加载所有 .md 格式的 Skill 文件
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { readdir, readFile } from "node:fs/promises";
import { join, extname } from "node:path";
import { createHash } from "node:crypto";
import type { LearnedSkill, SkillMetadata } from "./types.js";

/**
 * 从指定目录加载所有 Skill 文件
 *
 * @param dir - Skill 目录路径
 * @returns 已加载的 Skill 列表
 */
export async function loadSkillsFromDir(dir: string): Promise<LearnedSkill[]> {
  let files: string[];
  try {
    files = await readdir(dir);
  } catch {
    return [];
  }

  const skills: LearnedSkill[] = [];

  for (const filename of files) {
    if (extname(filename) !== ".md") continue;
    const filepath = join(dir, filename);
    try {
      const content = await readFile(filepath, "utf-8");
      const skill = parseSkillFile(filepath, filename, content);
      if (skill) skills.push(skill);
    } catch {
      // 解析失败的文件静默跳过
    }
  }

  return skills;
}

/**
 * 解析单个 Skill 文件
 */
function parseSkillFile(
  filepath: string,
  filename: string,
  content: string,
): LearnedSkill | null {
  const match = content.match(/^---\n([\s\S]*?)\n---\n?([\s\S]*)$/);
  if (!match) return null;

  const [, yamlStr, body] = match;
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
      createdAt: metadata.createdAt ?? new Date().toISOString(),
      source: (metadata.source as LearnedSkill["metadata"]["source"]) ?? "manual",
      quality: Number(metadata.quality ?? 0),
      usageCount: Number(metadata.usageCount ?? 0),
      tags: parseTriggers(metadata.tags),
    } satisfies SkillMetadata,
    content: body.trim(),
    contentHash,
    priority: filepath.includes(".claude/skills") ? 10 : 5,
  };
}

/** 简单 YAML 解析（key: value 格式） */
function parseYaml(yaml: string): Record<string, string> {
  const result: Record<string, string> = {};
  for (const line of yaml.split("\n")) {
    const idx = line.indexOf(":");
    if (idx === -1) continue;
    const key = line.slice(0, idx).trim();
    const value = line.slice(idx + 1).trim().replace(/^["']|["']$/g, "");
    if (key) result[key] = value;
  }
  return result;
}

/** 解析逗号分隔的触发词列表 */
function parseTriggers(value: string | undefined): string[] {
  if (!value) return [];
  return value.split(",").map((s) => s.trim()).filter(Boolean);
}
