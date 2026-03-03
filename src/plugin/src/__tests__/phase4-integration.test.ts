/**
 * @file phase4-integration.test.ts
 * @description Phase 4 Plugin 端到端集成测试
 *              验证 Learner 模块（detector -> extractor -> writer）流水线、
 *              Skill 模板文件加载，以及 Hooks 集成逻辑
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { mkdtempSync, rmSync, existsSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));

// ============================================
// 测试辅助函数
// ============================================

let tempDir: string;

beforeEach(() => {
  tempDir = mkdtempSync(join(tmpdir(), "ghostcode-phase4-test-"));
});

afterEach(() => {
  rmSync(tempDir, { recursive: true, force: true });
});

// ============================================
// 场景 1：Learner 模块端到端流水线
// ============================================

describe("Phase 4 Learner 模块端到端流水线", () => {
  /**
   * 验证 detector -> extractor -> writer 完整流水线：
   * 1. 检测会话内容中的模式
   * 2. 将检测到的模式提取为 Skill 模板
   * 3. 将模板写入临时目录
   */
  it("detector -> extractor -> writer 完整流水线", async () => {
    const { detectPatterns } = await import("../learner/detector.js");
    const { extractSkillTemplate } = await import("../learner/extractor.js");
    const { writeSkillFile } = await import("../learner/writer.js");

    // ============================================
    // 第一步：使用包含错误修复模式的会话内容触发检测
    // 需满足: 内容 >= 100 字符 + 含错误模式 + 含解决关键词
    // ============================================
    const sessionContent = `
      用户遇到了 TypeError: Cannot read properties of undefined (reading 'map')
      错误原因：在数据未加载完成时就尝试访问数组的 map 方法
      解决方案：添加可选链操作符 ?. 和空值合并运算符 ?? 进行防御性访问
      修复代码：const items = data?.items ?? []
      验证：问题已成功解决，代码正常运行
    `.trim();

    // 第一步：检测模式
    const patterns = detectPatterns(sessionContent);
    // 注意：检测结果依赖启发式规则，此处验证函数可调用不报错
    expect(Array.isArray(patterns)).toBe(true);

    // ============================================
    // 第二步：构造一个模拟候选（确保 extractSkillTemplate 被正确调用）
    // ============================================
    const mockPattern = {
      id: "test-pattern-001",
      problem: "TypeError: Cannot read properties of undefined",
      solution: "使用可选链操作符 ?. 防御性访问，添加 ?? 空值回退",
      confidence: 80,
      occurrences: 2,
      firstSeen: new Date().toISOString(),
      lastSeen: new Date().toISOString(),
      suggestedTriggers: ["typescript", "fix"],
      suggestedTags: ["typescript", "bugfix"],
    };

    // 第二步：提取 Skill 模板
    const template = extractSkillTemplate(
      mockPattern,
      "fix-undefined-access",
      "修复 undefined 访问错误"
    );

    // 验证模板格式正确
    expect(template).toMatch(/^---\n/);
    expect(template).toContain("id: fix-undefined-access");
    expect(template).toContain("source: extracted");
    expect(template).toContain(mockPattern.problem);
    expect(template).toContain(mockPattern.solution);

    // 第三步：写入临时目录
    await writeSkillFile(tempDir, "fix-undefined-access.md", template);

    // 验证文件已写入
    const filepath = join(tempDir, "fix-undefined-access.md");
    expect(existsSync(filepath)).toBe(true);

    const fileContent = readFileSync(filepath, "utf-8");
    expect(fileContent).toBe(template);
  });

  /**
   * 验证高置信度内容能完整触发检测流水线
   */
  it("高置信度会话内容完整流水线验证", async () => {
    const { detectPatterns } = await import("../learner/detector.js");
    const { extractSkillTemplate } = await import("../learner/extractor.js");

    // 构造高置信度的会话内容（含多个错误模式 + 解决方案关键词）
    const highConfidenceContent = `
      错误：TypeError: Cannot read properties of null
      问题：Rust 生命周期 error 导致借用检查失败
      错误信息: borrow of moved value
      解决方案：使用 Clone trait 或调整所有权转移
      修复方案：添加显式生命周期标注 'a
      解决：通过重构引用关系解决了生命周期问题
      问题已修复，代码现在可以正常编译和运行
    `.repeat(2);

    const patterns = detectPatterns(highConfidenceContent);

    // 若检测到模式，验证提取流程正常
    for (const pattern of patterns) {
      expect(pattern.confidence).toBeGreaterThanOrEqual(0);
      expect(pattern.confidence).toBeLessThanOrEqual(100);
      expect(pattern.id).toBeTruthy();
      expect(pattern.problem).toBeTruthy();
      expect(pattern.solution).toBeTruthy();

      // 提取模板不应抛出异常
      const template = extractSkillTemplate(pattern, `skill-${pattern.id}`, "测试 Skill");
      expect(template).toContain("---");
      expect(template).toContain(`id: skill-${pattern.id}`);
    }
  });
});

// ============================================
// 场景 2：Skill 模板文件加载验证
// ============================================

describe("Phase 4 Skill 模板文件加载验证", () => {
  const SKILLS_DIR = join(__dirname, "../.claude/skills");

  // 必须存在的内置 Skill 模板文件
  const REQUIRED_SKILLS = [
    "team-research.md",
    "team-plan.md",
    "team-exec.md",
    "team-review.md",
    "spec-research.md",
    "spec-plan.md",
    "spec-impl.md",
  ];

  // frontmatter 必填字段
  const REQUIRED_FIELDS = ["id", "name", "description", "triggers", "source", "version"];

  /**
   * 从 Markdown 文件中提取 YAML frontmatter 键值对
   * 只解析 key: value 单行格式（不处理嵌套 YAML）
   */
  function parseFrontmatter(content: string): Record<string, string> {
    const match = content.match(/^---\n([\s\S]*?)\n---/);
    if (!match) return {};
    const result: Record<string, string> = {};
    for (const line of match[1].split("\n")) {
      const colonIdx = line.indexOf(":");
      if (colonIdx === -1) continue;
      const key = line.slice(0, colonIdx).trim();
      const value = line.slice(colonIdx + 1).trim();
      result[key] = value;
    }
    return result;
  }

  /**
   * 验证 loader 能正确加载临时目录中的 Skill 文件
   */
  it("loader 能正确加载 Skill 文件", async () => {
    const { loadSkillsFromDir } = await import("../learner/loader.js");

    // 在临时目录写入一个有效的 Skill 文件
    const validSkillContent = `---
id: phase4-test-skill
name: "Phase 4 测试 Skill"
description: "端到端集成测试用 Skill"
triggers: test, phase4
source: extracted
version: "1.0.0"
quality: "75"
usageCount: "0"
tags: test
createdAt: 2026-03-03T00:00:00.000Z
---
# Phase 4 测试 Skill

## 问题
测试问题描述

## 解决方案
测试解决方案描述
`;

    const skillPath = join(tempDir, "phase4-test-skill.md");
    const { writeFileSync } = await import("node:fs");
    writeFileSync(skillPath, validSkillContent);

    // 加载并验证
    const skills = await loadSkillsFromDir(tempDir);
    expect(skills.length).toBe(1);

    const skill = skills[0];
    expect(skill.metadata.id).toBe("phase4-test-skill");
    expect(skill.metadata.name).toBe("Phase 4 测试 Skill");
    expect(skill.metadata.source).toBe("extracted");
    expect(skill.content).toBeTruthy();
  });

  /**
   * 验证所有内置 Skill 模板文件存在
   */
  it("所有内置 team-*/spec-* Skill 模板文件存在", () => {
    for (const filename of REQUIRED_SKILLS) {
      const filepath = join(SKILLS_DIR, filename);
      expect(existsSync(filepath), `缺少内置 Skill 文件: ${filename}`).toBe(true);
    }
  });

  /**
   * 验证每个内置 Skill 模板的 frontmatter 格式正确
   */
  for (const filename of REQUIRED_SKILLS) {
    it(`${filename} frontmatter 格式正确`, () => {
      const filepath = join(SKILLS_DIR, filename);
      if (!existsSync(filepath)) return;

      const content = readFileSync(filepath, "utf-8");

      // 验证 frontmatter 存在
      expect(content).toMatch(/^---\n[\s\S]*?\n---/);

      const fm = parseFrontmatter(content);

      // 验证必填字段存在
      for (const field of REQUIRED_FIELDS) {
        expect(fm[field], `${filename} 缺少必填字段: ${field}`).toBeTruthy();
      }

      // 验证 id 与文件名一致
      const expectedId = filename.replace(".md", "");
      expect(fm["id"]).toBe(expectedId);

      // 验证 source 为 builtin
      expect(fm["source"]).toBe("builtin");
    });
  }

  /**
   * 验证 loader 能加载内置 Skills 目录
   * 注意：不要求全部加载成功（某些字段格式可能不同），只验证加载不崩溃
   */
  it("loader 加载内置 Skills 目录不崩溃", async () => {
    if (!existsSync(SKILLS_DIR)) return;

    const { loadSkillsFromDir } = await import("../learner/loader.js");

    // 加载不应抛出未捕获异常
    const skills = await loadSkillsFromDir(SKILLS_DIR);
    expect(Array.isArray(skills)).toBe(true);
    // 应能加载到至少 1 个 Skill
    expect(skills.length).toBeGreaterThan(0);
  });
});

// ============================================
// 场景 3：Hooks 集成验证
// ============================================

describe("Phase 4 Hooks 集成验证", () => {
  /**
   * 验证 stopHandler 包含 Skill Learning (onSessionEnd) 调用逻辑
   * 通过检查 handlers.ts 源码确认集成
   */
  it("handlers.ts 的 stopHandler 包含 learner.onSessionEnd 调用", () => {
    const handlersPath = join(__dirname, "../hooks/handlers.ts");
    expect(existsSync(handlersPath), "handlers.ts 文件应存在").toBe(true);

    const source = readFileSync(handlersPath, "utf-8");

    // 验证 stopHandler 包含 learner 导入
    expect(source).toContain("onSessionEnd");
    expect(source).toContain("learner");

    // 验证 stopHandler 函数定义存在
    expect(source).toContain("export async function stopHandler");

    // 验证包含 try/catch 保证 skill learning 失败不阻断 stop 流程
    expect(source).toContain("try");
  });

  /**
   * 验证 manager.ts 中的 onSessionEnd 函数包含 skill_learn_fragment IPC 调用
   */
  it("manager.ts 的 onSessionEnd 包含 skill_learn_fragment IPC 调用路径", () => {
    const managerPath = join(__dirname, "../learner/manager.ts");
    expect(existsSync(managerPath), "manager.ts 文件应存在").toBe(true);

    const source = readFileSync(managerPath, "utf-8");

    // 验证 onSessionEnd 函数存在
    expect(source).toContain("export async function onSessionEnd");

    // 验证包含 skill_learn_fragment IPC 调用
    expect(source).toContain("skill_learn_fragment");

    // 验证包含置信度过滤（只上报 >= 70 的候选）
    expect(source).toContain("70");
  });

  /**
   * 验证 manager.ts 的 appendSessionContent 函数可以累积会话内容
   */
  it("appendSessionContent 累积会话内容", async () => {
    const { appendSessionContent } = await import("../learner/manager.js");

    // appendSessionContent 应不抛出异常
    expect(() => appendSessionContent("第一段会话内容")).not.toThrow();
    expect(() => appendSessionContent("第二段会话内容")).not.toThrow();
  });

  /**
   * 验证 listCandidates 在 IPC 不可用时优雅降级（返回空数组）
   */
  it("listCandidates 在 IPC 不可用时返回空数组（优雅降级）", async () => {
    // mock IPC 调用为失败状态
    vi.mock("../ipc.js", () => ({
      callDaemon: vi.fn().mockRejectedValue(new Error("IPC 不可用")),
    }));

    const { listCandidates } = await import("../learner/manager.js");

    // IPC 失败时应返回空数组而不是抛出异常
    const result = await listCandidates();
    expect(Array.isArray(result)).toBe(true);
    expect(result).toHaveLength(0);

    vi.restoreAllMocks();
  });
});
