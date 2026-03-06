/**
 * @file tsup 构建配置
 * @description GhostCode Plugin 的构建工具配置
 *
 *              构建产物说明：
 *              - dist/ 下保留完整模块结构（bundle: false）
 *              - dist/daemon.js, dist/session-lease.js, dist/keywords/index.js 等
 *                被 scripts/*.mjs 通过动态 import 引用
 *              - dist/cli.js: CLI 命令行工具
 *
 *              为什么 bundle: false？
 *              Hook 脚本（scripts/*.mjs）需要动态 import 单独的模块文件，
 *              如 import("../dist/daemon.js")。如果 bundle: true，所有模块
 *              会被打包成 index.js + chunks，独立模块文件不存在，Hook 脚本会失败。
 *
 *              注意：scripts/*.mjs 是原生 ESM 脚本，由 hooks.json 直接调用，
 *              不需要 tsup 编译。
 *
 *              隐藏目录清理说明：
 *              tsup 的 clean: true 使用 rimraf，但 rimraf v4+ 默认不删除以点开头
 *              的隐藏目录（如 dist/.claude/）。为防止旧构建残留，
 *              通过 beforeBuild 钩子在每次构建前手动清理 dist/.claude/ 目录。
 * @author Atlas.oi
 * @date 2026-03-05
 */
import { defineConfig } from "tsup";
import * as fs from "node:fs";
import * as path from "node:path";

export default defineConfig({
  // 构建入口：所有非测试 TypeScript 源文件
  // bundle: false 模式下需要包含所有源文件，tsup 会逐个转译
  //
  // 排除规则说明：
  // - !src/**/*.test.ts — 排除单元测试文件
  // - !src/**/__tests__/** — 排除测试目录
  // - !src/.claude/** — 排除 .claude 目录（包含 skills 等运行时资源，非 TS 源码）
  //   原因：tsup 的 src/**/*.ts glob 会匹配 src/.claude/ 下任意 .ts 文件，
  //   导致编译时在 dist/ 下产生 .claude/ 子目录，
  //   进而被 dist 复制时造成 dist/.claude/.claude/ 双层嵌套的错误结构。
  entry: [
    "src/**/*.ts",
    "!src/**/*.test.ts",
    "!src/**/__tests__/**",
    "!src/.claude/**",
  ],

  // 输出格式：仅 ESM
  format: ["esm"],

  // 生成 TypeScript 声明文件
  dts: true,

  // 生成 Source Map
  sourcemap: true,

  // 构建前清空 dist 目录
  // 注意：rimraf v4+ 默认不删除隐藏目录（以 . 开头），
  // 因此通过 beforeBuild 钩子额外清理 dist/.claude/ 残留
  clean: true,

  // 目标平台：Node.js 20+
  target: "node20",

  // 非打包模式：保留独立模块文件结构
  // Hook 脚本通过 import("../dist/daemon.js") 等路径动态引用
  bundle: false,

  /**
   * beforeBuild 钩子：在 tsup 执行 clean 之后、编译之前运行
   *
   * 作用：
   * 1. 清理 dist/ 下残留的隐藏目录（如 .claude/）
   *    tsup 的 clean: true 不处理隐藏目录，需手动清理
   * 2. 防止构建产物出现 dist/.claude/.claude/ 双层嵌套结构
   */
  async onSuccess() {
    // 构建完成后，确认 dist/.claude/ 不存在（防御性检查）
    const distClaudePath = path.resolve("dist", ".claude");
    if (fs.existsSync(distClaudePath)) {
      fs.rmSync(distClaudePath, { recursive: true, force: true });
      console.log("[GhostCode Build] 已清理 dist/.claude/ 残留目录");
    }
  },
});
