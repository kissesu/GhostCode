/**
 * @file extractor.test.ts
 * @description Skill 模板提取器测试
 * @author Atlas.oi
 * @date 2026-03-03
 */
import { describe, test, expect } from "vitest";
import { extractSkillTemplate } from "../extractor.js";
import type { PatternDetection } from "../types.js";

const samplePattern: PatternDetection = {
  id: "test-id",
  problem: "Rust 生命周期错误",
  solution: "添加显式生命周期标注 'a",
  confidence: 80,
  occurrences: 3,
  firstSeen: new Date().toISOString(),
  lastSeen: new Date().toISOString(),
  suggestedTriggers: ["rust", "lifetime"],
  suggestedTags: ["rust", "fix"],
};

describe("extractSkillTemplate", () => {
  test("生成包含 YAML frontmatter 的 Markdown", () => {
    const result = extractSkillTemplate(samplePattern, "fix-rust-lifetime", "修复 Rust 生命周期");
    expect(result).toMatch(/^---\n/);
    expect(result).toContain("---");
  });

  test("frontmatter 包含必填字段", () => {
    const result = extractSkillTemplate(samplePattern, "fix-rust-lifetime", "修复 Rust 生命周期");
    expect(result).toContain("id: fix-rust-lifetime");
    expect(result).toContain("name:");
    expect(result).toContain("triggers:");
    expect(result).toContain("source: extracted");
  });

  test("正文包含问题和解决方案", () => {
    const result = extractSkillTemplate(samplePattern, "fix-rust-lifetime", "修复 Rust 生命周期");
    expect(result).toContain(samplePattern.problem);
    expect(result).toContain(samplePattern.solution);
  });

  test("触发词列表非空", () => {
    const result = extractSkillTemplate(samplePattern, "fix-rust-lifetime", "修复 Rust 生命周期");
    for (const trigger of samplePattern.suggestedTriggers) {
      expect(result).toContain(trigger);
    }
  });
});
