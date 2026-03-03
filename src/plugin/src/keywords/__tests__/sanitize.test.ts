/**
 * @file sanitize.test.ts
 * @description sanitizeForKeywordDetection 函数的单元测试
 * 验证代码块、行内代码、URL、文件路径、XML 标签等噪声移除逻辑
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { describe, it, expect } from "vitest";
import { sanitizeForKeywordDetection } from "../sanitize.js";

describe("sanitizeForKeywordDetection", () => {
  // ============================================
  // 测试 1：移除 Markdown 代码块（```...```）中的内容
  // 代码块内的关键词不应触发检测
  // ============================================
  it("移除 Markdown 代码块内容", () => {
    const input = "普通文本 ```ralph autopilot``` 更多文本";
    const result = sanitizeForKeywordDetection(input);
    expect(result).not.toContain("ralph");
    expect(result).not.toContain("autopilot");
    expect(result).toContain("普通文本");
  });

  it("移除多行代码块内容", () => {
    const input = "开始\n```\nconst ralph = 'ultrawork';\n```\n结束";
    const result = sanitizeForKeywordDetection(input);
    expect(result).not.toContain("ralph");
    expect(result).not.toContain("ultrawork");
    expect(result).toContain("开始");
    expect(result).toContain("结束");
  });

  // ============================================
  // 测试 2：移除行内代码（`...`）中的内容
  // ============================================
  it("移除行内代码内容", () => {
    const input = "请执行 `ralph` 命令";
    const result = sanitizeForKeywordDetection(input);
    expect(result).not.toContain("ralph");
    expect(result).toContain("请执行");
    expect(result).toContain("命令");
  });

  // ============================================
  // 测试 3：移除 URL（https://...）
  // URL 中可能含有关键词，不应误触发
  // ============================================
  it("移除 HTTPS URL", () => {
    const input = "参考文档 https://example.com/ralph/autopilot/docs 了解更多";
    const result = sanitizeForKeywordDetection(input);
    // URL 已被移除，关键词不应出现
    expect(result).not.toContain("https://example.com/ralph/autopilot/docs");
    expect(result).toContain("参考文档");
    expect(result).toContain("了解更多");
  });

  it("移除 HTTP URL", () => {
    const input = "访问 http://api.example.com/v1/team/status 获取状态";
    const result = sanitizeForKeywordDetection(input);
    expect(result).not.toContain("http://api.example.com");
    expect(result).toContain("访问");
  });

  // ============================================
  // 测试 4：移除文件路径（/foo/bar/baz）
  // 路径中含有关键词片段不应误触发
  // ============================================
  it("移除绝对文件路径", () => {
    const input = "请读取 /home/user/project/ralph/config.json 配置文件";
    const result = sanitizeForKeywordDetection(input);
    expect(result).not.toContain("/home/user/project/ralph/config.json");
    expect(result).toContain("请读取");
    expect(result).toContain("配置文件");
  });

  it("移除相对文件路径", () => {
    const input = "修改 ./src/hooks/autopilot/index.ts 文件";
    const result = sanitizeForKeywordDetection(input);
    expect(result).not.toContain("./src/hooks/autopilot/index.ts");
    expect(result).toContain("修改");
    expect(result).toContain("文件");
  });

  // ============================================
  // 测试 5：移除 XML 标签（<tag>...</tag>）
  // ============================================
  it("移除 XML 标签及其内容", () => {
    const input = "文本 <mode>ralph autopilot</mode> 结束";
    const result = sanitizeForKeywordDetection(input);
    expect(result).not.toContain("ralph");
    expect(result).not.toContain("autopilot");
    expect(result).toContain("文本");
    expect(result).toContain("结束");
  });

  it("移除自闭合 XML 标签", () => {
    const input = "设置 <ralph-mode enabled=\"true\" /> 开始";
    const result = sanitizeForKeywordDetection(input);
    expect(result).toContain("设置");
    expect(result).toContain("开始");
  });

  // ============================================
  // 测试 6：保留普通文本不变
  // 正常文本不应被误修改
  // ============================================
  it("保留普通文本内容", () => {
    const input = "这是一段普通文本，没有任何特殊标记或链接";
    const result = sanitizeForKeywordDetection(input);
    expect(result).toBe(input);
  });

  it("保留包含关键词的普通文本", () => {
    const input = "请使用 ralph 模式处理这个任务";
    const result = sanitizeForKeywordDetection(input);
    // 普通文本中的关键词应被保留（sanitize 不移除普通关键词）
    expect(result).toContain("ralph");
    expect(result).toContain("请使用");
    expect(result).toContain("模式处理这个任务");
  });

  // ============================================
  // 测试 7：处理混合输入
  // 综合场景：代码块 + URL + 路径 + XML + 普通文本同时存在
  // ============================================
  it("处理混合输入：仅移除噪声保留关键词所在普通文本", () => {
    const input = [
      "用户说: ralph 请帮我",
      "```",
      "autopilot ultrawork",
      "```",
      "然后访问 https://example.com/team/api",
      "读取 /etc/ghostcode/config.toml",
      "<system-context>cancel now</system-context>",
      "最后执行 ultrawork 任务",
    ].join("\n");

    const result = sanitizeForKeywordDetection(input);

    // 代码块内的关键词应被移除
    expect(result).not.toContain("autopilot");
    // XML 标签内的关键词应被移除
    expect(result).not.toContain("cancel");
    // 普通文本中的关键词应被保留
    expect(result).toContain("ralph");
    expect(result).toContain("ultrawork");
  });
});
