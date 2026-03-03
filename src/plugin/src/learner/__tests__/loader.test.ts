/**
 * @file loader.test.ts
 * @description Skill 文件加载器测试
 * @author Atlas.oi
 * @date 2026-03-03
 */
import { describe, test, expect, beforeEach, afterEach } from "vitest";
import { loadSkillsFromDir } from "../loader.js";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

let tempDir: string;

beforeEach(() => {
  tempDir = mkdtempSync(join(tmpdir(), "ghostcode-loader-test-"));
});

afterEach(() => {
  rmSync(tempDir, { recursive: true, force: true });
});

const VALID_SKILL = `---
id: test-skill
name: Test Skill
description: 测试 Skill
triggers: test, testing
source: extracted
version: "1.0.0"
quality: "80"
usageCount: "0"
tags: test
createdAt: 2026-03-03T00:00:00.000Z
---
# Test Skill
这是测试内容。`;

describe("loadSkillsFromDir", () => {
  test("空目录返回空数组", async () => {
    const skills = await loadSkillsFromDir(tempDir);
    expect(skills).toEqual([]);
  });

  test("加载有效的 Skill 文件", async () => {
    writeFileSync(join(tempDir, "test-skill.md"), VALID_SKILL);
    const skills = await loadSkillsFromDir(tempDir);
    expect(skills.length).toBe(1);
    expect(skills[0].metadata.id).toBe("test-skill");
  });

  test("忽略非 .md 文件", async () => {
    writeFileSync(join(tempDir, "test.txt"), "not a skill");
    writeFileSync(join(tempDir, "test.json"), "{}");
    const skills = await loadSkillsFromDir(tempDir);
    expect(skills.length).toBe(0);
  });

  test("加载多个 Skill 文件", async () => {
    writeFileSync(join(tempDir, "skill1.md"), VALID_SKILL);
    const skill2 = VALID_SKILL.replace("id: test-skill", "id: skill2").replace("name: Test Skill", "name: Skill 2");
    writeFileSync(join(tempDir, "skill2.md"), skill2);
    const skills = await loadSkillsFromDir(tempDir);
    expect(skills.length).toBe(2);
  });
});
