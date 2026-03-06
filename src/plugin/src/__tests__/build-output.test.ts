/**
 * @file build-output.test.ts
 * @description 构建产物结构验证测试
 *              测试 dist/ 目录不存在 .claude/.claude/ 嵌套路径
 *              测试 package.json 包含 postinstall script
 *              测试 pack-plugin.sh 输出包的 skills/ 目录结构正确
 * @author Atlas.oi
 * @date 2026-03-05
 */

import { describe, it, expect } from "vitest";
import * as fs from "node:fs";
import * as path from "node:path";

// 从测试文件位置向上推导项目根目录
// src/__tests__/build-output.test.ts → plugin 根目录
const PLUGIN_ROOT = path.resolve(__dirname, "../../");
const DIST_DIR = path.join(PLUGIN_ROOT, "dist");
const PACKAGE_JSON_PATH = path.join(PLUGIN_ROOT, "package.json");

describe("构建产物结构验证", () => {
  // ============================================
  // 测试 1: dist/ 下不存在 .claude/.claude/ 嵌套路径
  // 根因: tsup entry 使用 src/**/*.ts 时，
  //       src/.claude/ 目录被错误地编译进 dist/
  //       导致 dist/.claude/.claude/ 双层嵌套
  // ============================================
  it("dist/ 下不应存在 .claude/.claude/ 嵌套路径", () => {
    // 如果 dist 目录不存在则跳过（未构建状态）
    if (!fs.existsSync(DIST_DIR)) {
      console.warn("dist/ 目录不存在，跳过测试（请先运行 pnpm build）");
      return;
    }

    const nestedClaudePath = path.join(DIST_DIR, ".claude", ".claude");
    const nestedExists = fs.existsSync(nestedClaudePath);

    expect(nestedExists, `dist/.claude/.claude/ 嵌套路径不应存在，但发现了: ${nestedClaudePath}`).toBe(false);
  });

  // ============================================
  // 测试 2: dist/ 下不应存在任何 .claude 目录
  // tsup 构建时 src/.claude/ 目录不应被编译进 dist/
  // ============================================
  it("dist/ 下不应存在任何 .claude 目录", () => {
    if (!fs.existsSync(DIST_DIR)) {
      console.warn("dist/ 目录不存在，跳过测试（请先运行 pnpm build）");
      return;
    }

    const claudeInDist = path.join(DIST_DIR, ".claude");
    const claudeExists = fs.existsSync(claudeInDist);

    expect(claudeExists, `dist/.claude/ 不应存在，但发现了: ${claudeInDist}`).toBe(false);
  });

  // ============================================
  // 测试 3: package.json 必须包含 postinstall script
  // postinstall 在 npm install 后自动执行，用于设置环境
  // ============================================
  it("package.json 应包含 postinstall script", () => {
    expect(fs.existsSync(PACKAGE_JSON_PATH), `package.json 不存在: ${PACKAGE_JSON_PATH}`).toBe(true);

    const raw = fs.readFileSync(PACKAGE_JSON_PATH, "utf-8");
    const pkg = JSON.parse(raw) as Record<string, unknown>;

    expect(pkg).toHaveProperty("scripts");

    const scripts = pkg["scripts"] as Record<string, string | undefined>;
    expect(scripts).toHaveProperty("postinstall");

    // 获取 postinstall 值，通过先检查避免 TS2532 undefined 错误
    const postinstall = scripts["postinstall"];
    expect(typeof postinstall).toBe("string");
    expect((postinstall ?? "").length, "postinstall script 不能为空").toBeGreaterThan(0);
  });

  // ============================================
  // 测试 4: tsup.config.ts 应排除 .claude 目录
  // 验证 tsup 配置文件中存在排除 .claude 的规则
  // ============================================
  it("tsup.config.ts 应包含排除 .claude 目录的配置", () => {
    const tsupConfigPath = path.join(PLUGIN_ROOT, "tsup.config.ts");
    expect(fs.existsSync(tsupConfigPath), `tsup.config.ts 不存在: ${tsupConfigPath}`).toBe(true);

    const content = fs.readFileSync(tsupConfigPath, "utf-8");

    // 验证 entry 中有排除 .claude 的规则
    const hasClaudeExclusion = content.includes('!src/.claude') || content.includes('!src/**/.claude');
    expect(hasClaudeExclusion, "tsup.config.ts 应在 entry 中排除 .claude 目录（如 !src/.claude/**）").toBe(true);
  });
});

describe("pack-plugin.sh skills 目录结构验证", () => {
  // ============================================
  // 测试 5: Plugin 源目录的 skills/ 目录结构正确
  // pack-plugin.sh 会从此目录复制 skills 到输出包
  // ============================================
  it("src/plugin/skills/ 目录应包含所有预期的 skill 子目录", () => {
    const skillsDir = path.join(PLUGIN_ROOT, "skills");

    // 七个标准 skill，pack-plugin.sh 要验证这些
    const expectedSkills = [
      "team-research",
      "team-plan",
      "team-exec",
      "team-review",
      "spec-research",
      "spec-plan",
      "spec-impl",
    ];

    if (!fs.existsSync(skillsDir)) {
      // skills 目录不存在时给出明确错误
      expect.fail(`skills/ 目录不存在: ${skillsDir}，pack-plugin.sh 将无法复制 skills`);
      return;
    }

    for (const skill of expectedSkills) {
      const skillPath = path.join(skillsDir, skill, "SKILL.md");
      expect(
        fs.existsSync(skillPath),
        `缺少 skills/${skill}/SKILL.md，pack-plugin.sh 验证会失败`
      ).toBe(true);
    }
  });
});
