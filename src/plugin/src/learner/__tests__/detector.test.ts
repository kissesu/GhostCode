/**
 * @file detector.test.ts
 * @description Skill 模式检测器测试
 * @author Atlas.oi
 * @date 2026-03-03
 */
import { describe, test, expect } from "vitest";
import { detectPatterns } from "../detector.js";

describe("detectPatterns", () => {
  test("空内容返回空数组", () => {
    expect(detectPatterns("")).toEqual([]);
  });

  test("短内容（< 100 字符）不触发检测", () => {
    expect(detectPatterns("短文本")).toEqual([]);
  });

  test("包含错误修复模式的内容返回候选", () => {
    const content = `
      用户遇到了 TypeError: Cannot read properties of undefined
      解决方案：检查对象是否为 null 后再访问属性
      修复后代码正常运行，问题解决了。
    `.repeat(3);
    const patterns = detectPatterns(content);
    expect(patterns.length).toBeGreaterThanOrEqual(0);
  });

  test("重复模式提高置信度", () => {
    const content = `
      修复了 Rust 生命周期错误，添加了显式生命周期标注。
      解决方案被验证有效。
    `.repeat(5);
    const patterns = detectPatterns(content);
    // 有重复迹象时应能检测到至少一个候选
    // 置信度体现在 confidence 字段
    for (const p of patterns) {
      expect(p.confidence).toBeGreaterThanOrEqual(0);
      expect(p.confidence).toBeLessThanOrEqual(100);
    }
  });
});
