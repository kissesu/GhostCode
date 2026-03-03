/**
 * @file 命令模板引擎测试
 * @description 测试 renderTemplate 变量替换、buildTaskPrompt 代码主权规则注入等功能
 * @author Atlas.oi
 * @date 2026-03-02
 */

import { describe, test, expect } from "vitest";
import { renderTemplate, buildTaskPrompt, SOVEREIGNTY_RULE } from "../templates";

describe("renderTemplate", () => {
  test("替换 {{WORKDIR}} 变量", () => {
    const template = "工作目录: {{WORKDIR}}";
    const result = renderTemplate(template, { WORKDIR: "/home/user/project" });
    expect(result).toBe("工作目录: /home/user/project");
  });

  test("替换多个变量", () => {
    const template = "后端: {{BACKEND}}，模型: {{MODEL}}，工作目录: {{WORKDIR}}";
    const result = renderTemplate(template, {
      BACKEND: "claude",
      MODEL: "claude-opus-4-6",
      WORKDIR: "/workspace",
    });
    expect(result).toBe("后端: claude，模型: claude-opus-4-6，工作目录: /workspace");
  });

  test("未知变量保持原样", () => {
    // {{UNKNOWN}} 不被删除也不报错，原样保留
    const template = "已知: {{WORKDIR}}，未知: {{UNKNOWN}}";
    const result = renderTemplate(template, { WORKDIR: "/workspace" });
    expect(result).toBe("已知: /workspace，未知: {{UNKNOWN}}");
  });

  test("保留 ROLE_FILE 引用不被干扰", () => {
    // ROLE_FILE: <path> 行不做变量替换干扰，仅作为普通文本保留
    const template = "ROLE_FILE: /roles/codex.md\n任务: {{WORKDIR}} 中执行";
    const result = renderTemplate(template, { WORKDIR: "/workspace" });
    expect(result).toBe("ROLE_FILE: /roles/codex.md\n任务: /workspace 中执行");
  });
});

describe("buildTaskPrompt", () => {
  test("非 claude 后端自动追加代码主权规则", () => {
    const result = buildTaskPrompt("请分析代码", "codex", { WORKDIR: "/workspace" });
    expect(result).toContain(SOVEREIGNTY_RULE);
  });

  test("gemini 后端自动追加代码主权规则", () => {
    const result = buildTaskPrompt("请分析代码", "gemini", { WORKDIR: "/workspace" });
    expect(result).toContain(SOVEREIGNTY_RULE);
  });

  test("claude 后端不追加代码主权规则", () => {
    const result = buildTaskPrompt("请分析代码", "claude", { WORKDIR: "/workspace" });
    expect(result).not.toContain(SOVEREIGNTY_RULE);
  });

  test("任务文本中的变量被正确替换", () => {
    const result = buildTaskPrompt(
      "在 {{WORKDIR}} 目录下，使用 {{BACKEND}} 后端执行任务",
      "claude",
      { WORKDIR: "/workspace", BACKEND: "claude" }
    );
    expect(result).toContain("在 /workspace 目录下，使用 claude 后端执行任务");
  });
});
