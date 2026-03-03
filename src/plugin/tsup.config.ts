/**
 * @file tsup 构建配置
 * @description GhostCode Plugin 的构建工具配置
 *              tsup 基于 esbuild，支持 ESM 输出和 TypeScript 声明文件生成
 * @author Atlas.oi
 * @date 2026-03-01
 */
import { defineConfig } from "tsup";

export default defineConfig({
  // 构建入口：Plugin 主入口 + HUD statusline CLI 入口
  entry: ["src/index.ts", "src/hud/statusline.ts"],

  // 输出格式：仅 ESM，不输出 CJS
  format: ["esm"],

  // 生成 TypeScript 声明文件（.d.ts）
  dts: true,

  // 生成 Source Map，便于调试
  sourcemap: true,

  // 构建前清空 dist 目录
  clean: true,

  // 目标平台：Node.js 20+
  target: "node20",

  // 打包模式：将所有本地模块打包进单一输出文件
  // bundle: false 会导致 daemon.js/ipc.js/hooks/index.js 不输出到 dist/
  bundle: true,
});
