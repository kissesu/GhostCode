/**
 * @file vitest.config.ts
 * @description Vitest 测试框架配置，使用 jsdom 模拟浏览器环境
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  test: {
    // 使用 jsdom 模拟浏览器 DOM 环境
    environment: 'jsdom',
    // 每个测试文件执行前自动引入 jest-dom 断言扩展
    setupFiles: ['./src/__tests__/setup.ts'],
    globals: true,
  },
});
