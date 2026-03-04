/**
 * @file install-contract.test.ts
 * @description 验证 package.json 分发产物契约
 *   确保 files 字段包含必要的发布目录，使 npm install 时能找到预编译二进制
 * @author Atlas.oi
 * @date 2026-03-04
 */

import { readFileSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it, expect } from "vitest";

// 计算 package.json 路径（从 src/__tests__ 向上三级到 plugin 根目录）
// 测试文件位置: src/plugin/src/__tests__/install-contract.test.ts
// package.json 位置: src/plugin/package.json
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const pkgPath = resolve(__dirname, "../../package.json");

describe("package.json 分发产物契约", () => {
  it("files 数组必须包含 bin 目录", () => {
    // 读取并解析 package.json
    const raw = readFileSync(pkgPath, "utf-8");
    const pkg = JSON.parse(raw) as { files?: string[] };

    // 验证 files 字段存在
    expect(pkg.files).toBeDefined();
    expect(Array.isArray(pkg.files)).toBe(true);

    // 验证包含 "bin" 目录，这是预编译二进制的存放位置
    // 如果缺少此项，npm install 时 install.ts 脚本无法找到二进制文件
    expect(pkg.files).toContain("bin");
  });

  it("files 数组至少包含 3 个项目（dist、.claude、bin）", () => {
    const raw = readFileSync(pkgPath, "utf-8");
    const pkg = JSON.parse(raw) as { files?: string[] };

    expect(pkg.files).toBeDefined();
    // 必须包含：dist（编译后 JS）、.claude（Claude Code 配置）、bin（预编译二进制）
    expect((pkg.files as string[]).length).toBeGreaterThanOrEqual(3);
  });

  it("files 数组必须包含 dist 目录", () => {
    const raw = readFileSync(pkgPath, "utf-8");
    const pkg = JSON.parse(raw) as { files?: string[] };

    expect(pkg.files).toContain("dist");
  });

  it("files 数组必须包含 .claude 目录", () => {
    const raw = readFileSync(pkgPath, "utf-8");
    const pkg = JSON.parse(raw) as { files?: string[] };

    expect(pkg.files).toContain(".claude");
  });
});
