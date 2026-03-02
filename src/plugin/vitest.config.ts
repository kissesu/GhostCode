/**
 * @file vitest 测试配置
 * @description GhostCode Plugin 的 vitest 测试框架配置
 * @author Atlas.oi
 * @date 2026-03-02
 */
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // 使用 Node.js 运行环境（IPC 测试需要 net 模块）
    environment: "node",
    // 全局 API（describe/it/expect）无需导入
    globals: false,
    // 测试文件匹配模式
    include: ["src/**/*.test.ts"],
    // 超时：默认 5s，p99 测试中个别用例会覆盖
    testTimeout: 5_000,
  },
});
