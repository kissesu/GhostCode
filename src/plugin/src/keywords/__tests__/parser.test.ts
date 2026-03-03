/**
 * @file parser.test.ts
 * @description Magic Keywords 解析器的单元测试
 * 验证 ralph/autopilot/team/ultrawork/cancel 关键词检测逻辑
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { describe, it, expect } from "vitest";
import { detectMagicKeywords, resolveKeywordPriority } from "../parser.js";

describe("detectMagicKeywords", () => {
  // ============================================
  // 测试 1：检测 ralph 关键词
  // ============================================
  it("检测 ralph 关键词", () => {
    const matches = detectMagicKeywords("请使用 ralph 模式");
    const types = matches.map((m) => m.type);
    expect(types).toContain("ralph");
  });

  it("ralph 不应误匹配连字符形式（如 ralph-mode）", () => {
    // 根据参考实现 /\b(ralph)\b(?!-)/ 排除 ralph-xxx 形式
    const matches = detectMagicKeywords("这是 ralph-mode 配置");
    const types = matches.map((m) => m.type);
    expect(types).not.toContain("ralph");
  });

  // ============================================
  // 测试 2：检测 autopilot 关键词（含变体）
  // 支持：autopilot, auto-pilot, auto pilot, full auto
  // ============================================
  it("检测 autopilot 关键词", () => {
    const matches = detectMagicKeywords("启用 autopilot 模式");
    const types = matches.map((m) => m.type);
    expect(types).toContain("autopilot");
  });

  it("检测 auto-pilot 变体", () => {
    const matches = detectMagicKeywords("开启 auto-pilot 功能");
    const types = matches.map((m) => m.type);
    expect(types).toContain("autopilot");
  });

  it("检测 full auto 变体", () => {
    const matches = detectMagicKeywords("请以 full auto 方式执行");
    const types = matches.map((m) => m.type);
    expect(types).toContain("autopilot");
  });

  // ============================================
  // 测试 3：检测 team 关键词（排除 "my team"、"the team"）
  // 避免误触发常见表达
  // ============================================
  it("检测 team 关键词", () => {
    const matches = detectMagicKeywords("启动 team 协作模式");
    const types = matches.map((m) => m.type);
    expect(types).toContain("team");
  });

  it('排除 "my team" 表达', () => {
    const matches = detectMagicKeywords("我的 my team 很棒");
    const types = matches.map((m) => m.type);
    expect(types).not.toContain("team");
  });

  it('排除 "the team" 表达', () => {
    const matches = detectMagicKeywords("请联系 the team 处理");
    const types = matches.map((m) => m.type);
    expect(types).not.toContain("team");
  });

  it('排除 "our team" 表达', () => {
    const matches = detectMagicKeywords("our team 已完成");
    const types = matches.map((m) => m.type);
    expect(types).not.toContain("team");
  });

  // ============================================
  // 测试 4：检测 ultrawork 关键词
  // ============================================
  it("检测 ultrawork 关键词", () => {
    const matches = detectMagicKeywords("使用 ultrawork 完成任务");
    const types = matches.map((m) => m.type);
    expect(types).toContain("ultrawork");
  });

  it("检测 ultrawork 缩写 ulw", () => {
    const matches = detectMagicKeywords("ulw 开始");
    const types = matches.map((m) => m.type);
    expect(types).toContain("ultrawork");
  });

  // ============================================
  // 测试 5：多关键词时返回所有匹配
  // ============================================
  it("多关键词时返回所有匹配项", () => {
    const matches = detectMagicKeywords("ralph 和 ultrawork 一起工作");
    const types = matches.map((m) => m.type);
    expect(types).toContain("ralph");
    expect(types).toContain("ultrawork");
  });

  // ============================================
  // 测试 6：无关键词时返回空数组
  // ============================================
  it("无关键词时返回空数组", () => {
    const matches = detectMagicKeywords("普通的开发任务，没有特殊模式");
    expect(matches).toHaveLength(0);
  });

  // ============================================
  // 测试 7：大小写不敏感
  // ============================================
  it("大小写不敏感匹配 RALPH", () => {
    const matches = detectMagicKeywords("请使用 RALPH 模式");
    const types = matches.map((m) => m.type);
    expect(types).toContain("ralph");
  });

  it("大小写不敏感匹配 AutoPilot", () => {
    const matches = detectMagicKeywords("AutoPilot 已启动");
    const types = matches.map((m) => m.type);
    expect(types).toContain("autopilot");
  });

  it("大小写不敏感匹配 ULTRAWORK", () => {
    const matches = detectMagicKeywords("ULTRAWORK 模式");
    const types = matches.map((m) => m.type);
    expect(types).toContain("ultrawork");
  });

  // ============================================
  // 测试 8：代码块中的关键词不触发
  // sanitize 先清理噪声，再检测
  // ============================================
  it("代码块内的关键词不触发检测", () => {
    const input = "查看这段代码 ```const mode = 'ralph';``` 是否正确";
    const matches = detectMagicKeywords(input);
    const types = matches.map((m) => m.type);
    expect(types).not.toContain("ralph");
  });
});

describe("resolveKeywordPriority", () => {
  // ============================================
  // 多关键词时返回最高优先级
  // 优先级：cancel(1) > ralph(2) > autopilot(3) > team(4) > ultrawork(5)
  // ============================================
  it("cancel 优先级最高", () => {
    const matches = detectMagicKeywords("cancel ralph ultrawork");
    const result = resolveKeywordPriority(matches);
    expect(result).not.toBeNull();
    expect(result!.type).toBe("cancel");
  });

  it("ralph 优先级高于 autopilot", () => {
    const matches = detectMagicKeywords("ralph autopilot");
    const result = resolveKeywordPriority(matches);
    expect(result).not.toBeNull();
    expect(result!.type).toBe("ralph");
  });

  it("autopilot 优先级高于 ultrawork", () => {
    const matches = detectMagicKeywords("autopilot ultrawork");
    const result = resolveKeywordPriority(matches);
    expect(result).not.toBeNull();
    expect(result!.type).toBe("autopilot");
  });

  it("无匹配时返回 null", () => {
    const result = resolveKeywordPriority([]);
    expect(result).toBeNull();
  });

  it("单个匹配时直接返回", () => {
    const matches = detectMagicKeywords("使用 ralph 模式");
    const result = resolveKeywordPriority(matches);
    expect(result).not.toBeNull();
    expect(result!.type).toBe("ralph");
  });
});
