/**
 * @file 路由策略测试
 * @description resolveRoute 函数的单元测试：关键词匹配、强制前缀覆盖、默认路由
 * @author Atlas.oi
 * @date 2026-03-02
 */

import { describe, it, expect } from 'vitest';
import { resolveRoute } from '../strategy';

describe('resolveRoute', () => {
  // 前端关键词 → gemini
  it('CSS task routes to gemini', () => {
    const result = resolveRoute('修改 CSS 样式让按钮更好看');
    expect(result.backend).toBe('gemini');
    expect(result.confidence).toBeGreaterThan(0);
  });

  it('UI component task routes to gemini', () => {
    const result = resolveRoute('创建一个响应式 UI 组件');
    expect(result.backend).toBe('gemini');
  });

  it('responsive design routes to gemini', () => {
    const result = resolveRoute('实现 responsive layout');
    expect(result.backend).toBe('gemini');
  });

  // 后端关键词 → codex
  it('API task routes to codex', () => {
    const result = resolveRoute('实现 REST API 接口');
    expect(result.backend).toBe('codex');
    expect(result.confidence).toBeGreaterThan(0);
  });

  it('database task routes to codex', () => {
    const result = resolveRoute('修改 database 查询逻辑');
    expect(result.backend).toBe('codex');
  });

  it('algorithm task routes to codex', () => {
    const result = resolveRoute('优化排序 algorithm');
    expect(result.backend).toBe('codex');
  });

  // 强制前缀覆盖
  it('explicit /codex prefix overrides auto', () => {
    const result = resolveRoute('/codex 修改 CSS 样式');
    expect(result.backend).toBe('codex');
    expect(result.confidence).toBe(1);
  });

  it('explicit /gemini prefix overrides auto', () => {
    const result = resolveRoute('/gemini 实现 API 接口');
    expect(result.backend).toBe('gemini');
    expect(result.confidence).toBe(1);
  });

  it('explicit /claude prefix overrides auto', () => {
    const result = resolveRoute('/claude 任意任务');
    expect(result.backend).toBe('claude');
    expect(result.confidence).toBe(1);
  });

  // 默认
  it('unknown task defaults to claude', () => {
    const result = resolveRoute('帮我看看这个问题');
    expect(result.backend).toBe('claude');
  });

  it('empty task defaults to claude', () => {
    const result = resolveRoute('');
    expect(result.backend).toBe('claude');
  });

  // 置信度
  it('keyword match has confidence > 0', () => {
    const result = resolveRoute('CSS');
    expect(result.confidence).toBeGreaterThan(0);
    expect(result.confidence).toBeLessThan(1);
  });

  it('forced prefix has confidence 1', () => {
    const result = resolveRoute('/codex anything');
    expect(result.confidence).toBe(1);
  });
});
