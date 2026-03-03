/**
 * @file skill-templates.test.ts
 * @description Skill 模板文件格式验证测试
 *              确保每个 Skill 模板具有有效的 YAML frontmatter 和必填字段
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { readFileSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, test, expect } from "vitest";

const __dirname = dirname(fileURLToPath(import.meta.url));
const SKILLS_DIR = join(__dirname, "../../.claude/skills");

// 必须存在的 Skill 模板文件列表
const REQUIRED_SKILLS = [
  "team-research.md",
  "team-plan.md",
  "team-exec.md",
  "team-review.md",
  "spec-research.md",
  "spec-plan.md",
  "spec-impl.md",
];

// 必填的 frontmatter 字段
const REQUIRED_FIELDS = ["id", "name", "description", "triggers", "source", "version"];

/**
 * 从 Markdown 文件中提取 YAML frontmatter 字段（简单解析）
 *
 * 业务逻辑说明：
 * 1. 匹配文件开头的 --- ... --- 块
 * 2. 逐行解析 key: value 格式
 *
 * @param {string} content - Markdown 文件内容
 * @returns {Record<string, unknown>} 解析后的 frontmatter 键值对
 */
function parseFrontmatter(content: string): Record<string, unknown> {
  const match = content.match(/^---\n([\s\S]*?)\n---/);
  if (!match) return {};
  const yaml = match[1];
  const result: Record<string, unknown> = {};
  for (const line of yaml.split("\n")) {
    const colonIdx = line.indexOf(":");
    if (colonIdx === -1) continue;
    const key = line.slice(0, colonIdx).trim();
    const value = line.slice(colonIdx + 1).trim();
    result[key] = value;
  }
  return result;
}

describe("Skill 模板文件格式验证", () => {
  test("所有必需的 Skill 文件存在", () => {
    for (const filename of REQUIRED_SKILLS) {
      const filepath = join(SKILLS_DIR, filename);
      expect(
        existsSync(filepath),
        `缺少 Skill 文件: ${filename}`
      ).toBe(true);
    }
  });

  for (const filename of REQUIRED_SKILLS) {
    describe(`${filename}`, () => {
      const filepath = join(SKILLS_DIR, filename);

      test("文件包含 YAML frontmatter", () => {
        if (!existsSync(filepath)) return;
        const content = readFileSync(filepath, "utf-8");
        expect(content).toMatch(/^---\n[\s\S]*?\n---/);
      });

      test("frontmatter 包含所有必填字段", () => {
        if (!existsSync(filepath)) return;
        const content = readFileSync(filepath, "utf-8");
        const fm = parseFrontmatter(content);
        for (const field of REQUIRED_FIELDS) {
          expect(fm[field], `${filename} 缺少必填字段: ${field}`).toBeTruthy();
        }
      });

      test("id 字段与文件名一致（去掉 .md 后缀）", () => {
        if (!existsSync(filepath)) return;
        const content = readFileSync(filepath, "utf-8");
        const fm = parseFrontmatter(content);
        const expectedId = filename.replace(".md", "");
        expect(fm["id"]).toBe(expectedId);
      });

      test("source 字段值为 builtin", () => {
        if (!existsSync(filepath)) return;
        const content = readFileSync(filepath, "utf-8");
        const fm = parseFrontmatter(content);
        expect(fm["source"]).toBe("builtin");
      });

      test("文件有正文内容（frontmatter 之后）", () => {
        if (!existsSync(filepath)) return;
        const content = readFileSync(filepath, "utf-8");
        const afterFm = content.replace(/^---\n[\s\S]*?\n---\n/, "").trim();
        expect(afterFm.length).toBeGreaterThan(0);
      });
    });
  }

  test("manifest.json 存在且格式正确", () => {
    const manifestPath = join(SKILLS_DIR, "manifest.json");
    expect(existsSync(manifestPath), "manifest.json 不存在").toBe(true);
    const manifest = JSON.parse(readFileSync(manifestPath, "utf-8")) as unknown;
    expect(Array.isArray(manifest)).toBe(true);
    const skills = manifest as Array<{ id: string; path: string; version: string }>;
    expect(skills.length).toBe(REQUIRED_SKILLS.length);
  });
});
