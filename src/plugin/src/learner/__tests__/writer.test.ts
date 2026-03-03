/**
 * @file writer.test.ts
 * @description Skill 文件写入器测试
 * @author Atlas.oi
 * @date 2026-03-03
 */
import { describe, test, expect, beforeEach, afterEach } from "vitest";
import { writeSkillFile } from "../writer.js";
import { mkdtempSync, rmSync, existsSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

let tempDir: string;

beforeEach(() => {
  tempDir = mkdtempSync(join(tmpdir(), "ghostcode-skill-test-"));
});

afterEach(() => {
  rmSync(tempDir, { recursive: true, force: true });
});

describe("writeSkillFile", () => {
  test("写入文件到指定目录", async () => {
    const content = "---\nid: test\n---\n# Test Skill";
    await writeSkillFile(tempDir, "test-skill.md", content);
    const filepath = join(tempDir, "test-skill.md");
    expect(existsSync(filepath)).toBe(true);
    expect(readFileSync(filepath, "utf-8")).toBe(content);
  });

  test("目录不存在时自动创建", async () => {
    const nestedDir = join(tempDir, "nested", "skills");
    const content = "---\nid: test\n---\n# Test";
    await writeSkillFile(nestedDir, "test.md", content);
    expect(existsSync(join(nestedDir, "test.md"))).toBe(true);
  });

  test("重复写入同名文件覆盖内容", async () => {
    await writeSkillFile(tempDir, "dup.md", "first content");
    await writeSkillFile(tempDir, "dup.md", "second content");
    const result = readFileSync(join(tempDir, "dup.md"), "utf-8");
    expect(result).toBe("second content");
  });
});
