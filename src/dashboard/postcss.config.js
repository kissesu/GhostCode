/**
 * @file postcss.config.js
 * @description PostCSS 配置，集成 Tailwind CSS 和 Autoprefixer
 * @author Atlas.oi
 * @date 2026-03-03
 */

export default {
  plugins: {
    // Tailwind CSS 处理器
    tailwindcss: {},
    // 自动添加浏览器前缀，提升兼容性
    autoprefixer: {},
  },
};
