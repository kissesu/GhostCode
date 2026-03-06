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
        // 禁用响应缓冲，确保 SSE（Server-Sent Events）流能实时推送到前端
        // 不设此选项会导致 SSE 响应被 http-proxy 缓冲，前端 EventSource 无法建立连接
        configure: (proxy) => {
          proxy.on('proxyRes', (proxyRes) => {
            // 检测 SSE 响应（content-type: text/event-stream），禁用缓冲
            const contentType = proxyRes.headers['content-type'] || '';
            if (contentType.includes('text/event-stream')) {
              // 设置 no-transform 防止中间层压缩或缓冲
              proxyRes.headers['cache-control'] = 'no-cache, no-transform';
            }
          });
        },
      },
    },
  },
  build: {
    // 输出到 dist/ 目录
    outDir: 'dist',
    sourcemap: false,
  },
});
