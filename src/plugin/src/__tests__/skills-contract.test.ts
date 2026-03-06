/**
 * @file skills-contract.test.ts
 * @description SKILL.md 文件合约测试：验证 7 个 skill 文件的结构完整性、内容充实度和格式规范
 * @author Atlas.oi
 * @date 2026-03-05
 */

import { describe, it, expect } from "vitest";
import * as fs from "node:fs";
import * as path from "node:path";

// ============================================
// 测试配置：7 个 skill 文件路径定义
// ============================================
// __dirname = src/plugin/src/__tests__，skills 在 src/plugin/skills/（上溯 2 层到 src/plugin/）
const SKILLS_DIR = path.resolve(__dirname, "../../skills");

const SKILL_NAMES = [
  "team-research",
  "team-plan",
  "team-exec",
  "team-review",
  "spec-research",
  "spec-plan",
  "spec-impl",
] as const;

const TEAM_SKILLS = ["team-research", "team-plan", "team-exec", "team-review"] as const;

// ============================================
// 辅助函数：读取 SKILL.md 文件内容
// ============================================
function readSkillFile(skillName: string): string {
  const filePath = path.join(SKILLS_DIR, skillName, "SKILL.md");
  return fs.readFileSync(filePath, "utf-8");
}

/**
 * 从 SKILL.md 中提取 frontmatter 字段（纯正则解析，无需 js-yaml 依赖）
 *
 * 业务逻辑说明：
 * 1. 匹配文件开头的 --- ... --- 块
 * 2. 逐行解析 key: value 格式的 YAML 字段
 * 3. 仅支持简单字符串值（frontmatter 中不会有嵌套对象）
 *
 * @param content - SKILL.md 文件的完整内容
 * @returns 解析后的 frontmatter 对象，若不存在则返回 null
 */
function extractFrontmatter(content: string): Record<string, string> | null {
  const match = content.match(/^---\n([\s\S]*?)\n---/);
  if (!match) return null;
  const result: Record<string, string> = {};
  const lines = match[1]!.split("\n");
  for (const line of lines) {
    const kv = line.match(/^(\w[\w_-]*):\s*['"]?(.*?)['"]?\s*$/);
    if (kv && kv[1] !== undefined && kv[2] !== undefined) {
      result[kv[1]] = kv[2];
    }
  }
  return Object.keys(result).length > 0 ? result : null;
}

/**
 * 检查内容是否包含指定节（支持多种写法）
 *
 * @param content - 文件内容
 * @param sectionNames - 候选节名称数组（任一匹配即可）
 * @returns 是否包含目标节
 */
function hasSection(content: string, sectionNames: string[]): boolean {
  return sectionNames.some((name) => {
    // 匹配 ## 标题格式 或 XML 标签格式 或下划线替换版本
    const patterns = [
      new RegExp(`^##\\s+${name}`, "m"),
      new RegExp(`<${name}>`, "i"),
      new RegExp(`^##\\s+${name.replace(/_/g, " ")}`, "mi"),
      new RegExp(`^##\\s+${name.replace(/ /g, "_")}`, "mi"),
    ];
    return patterns.some((p) => p.test(content));
  });
}

// ============================================
// 测试套件 1：frontmatter 基础字段验证
// 每个 SKILL.md 必须包含 name 和 description 字段
// ============================================
describe("SKILL.md frontmatter 基础字段验证", () => {
  for (const skillName of SKILL_NAMES) {
    it(`${skillName} - frontmatter 包含 name 字段`, () => {
      const content = readSkillFile(skillName);
      const frontmatter = extractFrontmatter(content);
      expect(frontmatter).not.toBeNull();
      expect(frontmatter).toHaveProperty("name");
      expect(typeof frontmatter!["name"]).toBe("string");
      expect((frontmatter!["name"] as string).length).toBeGreaterThan(0);
    });

    it(`${skillName} - frontmatter 包含 description 字段`, () => {
      const content = readSkillFile(skillName);
      const frontmatter = extractFrontmatter(content);
      expect(frontmatter).not.toBeNull();
      expect(frontmatter).toHaveProperty("description");
      expect(typeof frontmatter!["description"]).toBe("string");
      expect((frontmatter!["description"] as string).length).toBeGreaterThan(0);
    });
  }
});

// ============================================
// 测试套件 2：必要章节存在性验证
// 每个 skill 必须包含 Purpose、Use_When/Use When、Steps 节
// ============================================
describe("SKILL.md 必要章节存在性验证", () => {
  for (const skillName of SKILL_NAMES) {
    it(`${skillName} - 包含 Purpose 章节`, () => {
      const content = readSkillFile(skillName);
      const hasPurpose = hasSection(content, ["Purpose", "## Purpose"]);
      expect(hasPurpose).toBe(true);
    });

    it(`${skillName} - 包含 Use_When 或 Use When 章节`, () => {
      const content = readSkillFile(skillName);
      const hasUseWhen = hasSection(content, ["Use_When", "Use When", "Use_when"]);
      expect(hasUseWhen).toBe(true);
    });

    it(`${skillName} - 包含 Steps 章节`, () => {
      const content = readSkillFile(skillName);
      const hasSteps = hasSection(content, ["Steps", "## Steps"]);
      expect(hasSteps).toBe(true);
    });
  }
});

// ============================================
// 测试套件 3：内容充实度验证
// 每个 SKILL.md 行数必须 >= 80 行
// ============================================
describe("SKILL.md 内容充实度验证（行数 >= 80）", () => {
  for (const skillName of SKILL_NAMES) {
    it(`${skillName} - 文件行数 >= 80`, () => {
      const content = readSkillFile(skillName);
      const lineCount = content.split("\n").length;
      expect(lineCount).toBeGreaterThanOrEqual(80);
    });
  }
});

// ============================================
// 测试套件 4：team-* skill 专属章节验证
// team 类 skill 必须包含 Guardrails 和 Exit_Criteria 章节
// ============================================
describe("team-* SKILL.md 专属章节验证", () => {
  for (const skillName of TEAM_SKILLS) {
    it(`${skillName} - 包含 Guardrails 章节`, () => {
      const content = readSkillFile(skillName);
      const hasGuardrails = hasSection(content, ["Guardrails", "## Guardrails"]);
      expect(hasGuardrails).toBe(true);
    });

    it(`${skillName} - 包含 Exit_Criteria 或 Exit Criteria 章节`, () => {
      const content = readSkillFile(skillName);
      const hasExitCriteria = hasSection(content, ["Exit_Criteria", "Exit Criteria", "Exit criteria"]);
      expect(hasExitCriteria).toBe(true);
    });
  }
});

// ============================================
// 测试套件 5：PBT - frontmatter YAML 可解析性
// 基于属性的测试：所有 SKILL.md 的 frontmatter YAML 必须可正确解析
// ============================================
describe("PBT: SKILL.md frontmatter YAML 解析性验证", () => {
  it("所有 skill 的 frontmatter 均可正确解析为有效 YAML 对象", () => {
    const results = SKILL_NAMES.map((skillName) => {
      const content = readSkillFile(skillName);
      const frontmatter = extractFrontmatter(content);
      return { skillName, isValid: frontmatter !== null && typeof frontmatter === "object" };
    });

    // 所有 skill 都必须通过
    const failures = results.filter((r) => !r.isValid);
    expect(failures).toHaveLength(0);
  });

  it("所有 skill 的 frontmatter name 字段值与目录名匹配", () => {
    const mismatches = SKILL_NAMES.filter((skillName) => {
      const content = readSkillFile(skillName);
      const frontmatter = extractFrontmatter(content);
      if (!frontmatter) return true;
      const name = frontmatter["name"] as string;
      // name 必须等于 skillName 或包含 skillName 的核心部分
      return name !== skillName;
    });

    expect(mismatches).toHaveLength(0);
  });
});

// ============================================
// 测试套件 6：Do Not Use When 章节验证
// 每个 skill 应包含 Do Not Use When 描述
// ============================================
describe("SKILL.md Do Not Use When 章节验证", () => {
  for (const skillName of SKILL_NAMES) {
    it(`${skillName} - 包含 Do_Not_Use_When 或 Do Not Use When 章节`, () => {
      const content = readSkillFile(skillName);
      const hasDoNotUse = hasSection(content, [
        "Do_Not_Use_When",
        "Do Not Use When",
        "Do_not_use_when",
        "do_not_use_when",
      ]);
      expect(hasDoNotUse).toBe(true);
    });
  }
});
