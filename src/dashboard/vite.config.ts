/**
 * @file vite.config.ts
 * @description Vite 构建配置，支持 React + TypeScript，代理 API 请求到 ghostcode-web
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Vite 构建配置
export default defineConfig({
  plugins: [react()],
  server: {
    // 开发服务器代理：将 /api 请求转发到 ghostcode-web 后端
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:7070',
        changeOrigin: true,
      },
    },
  },
  build: {
    // 输出到 dist/ 目录
    outDir: 'dist',
    sourcemap: false,
  },
});
