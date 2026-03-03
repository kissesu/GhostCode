/**
 * @file tailwind.config.ts
 * @description Tailwind CSS 配置，启用 JIT 编译，扫描 src/ 下所有 TSX 文件
 * @author Atlas.oi
 * @date 2026-03-03
 */

import type { Config } from 'tailwindcss';

const config: Config = {
  // 扫描所有 TSX/TS 文件以生成所需的 CSS 类
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      // 暗色主题颜色变量扩展
      colors: {
        bg: {
          primary: '#0a0a0f',
          card: '#1a1a2e',
          hover: '#16213e',
        },
        text: {
          primary: '#e0e0ff',
          secondary: '#9090b0',
          muted: '#505070',
        },
        accent: {
          blue: '#4a9eff',
          green: '#4aff9e',
          red: '#ff4a4a',
          yellow: '#ffd04a',
          purple: '#b04aff',
        },
        border: {
          default: '#2a2a4a',
          subtle: '#1a1a3a',
        },
      },
    },
  },
  plugins: [],
};

export default config;
