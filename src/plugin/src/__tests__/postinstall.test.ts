/**
 * @file postinstall.test.ts
 * @description postinstall 脚本单元测试
 *   测试涵盖：CI 环境跳过、正常安装、离线回退、权限错误
 * @author Atlas.oi
 * @date 2026-03-04
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ============================================
// vi.mock 必须在顶层静态声明（vitest hoisting 机制）
// ============================================
vi.mock("../install.js", () => ({
  installFromRelease: vi.fn(),
}));

vi.mock("node:fs", async (importOriginal) => {
  const actual = await importOriginal<typeof import("node:fs")>();
  return {
    ...actual,
    existsSync: vi.fn(),
    readdirSync: vi.fn(),
    copyFileSync: vi.fn(),
    mkdirSync: vi.fn(),
    chmodSync: vi.fn(),
  };
});

import { installFromRelease } from "../install.js";
import { existsSync, readdirSync, copyFileSync, mkdirSync, chmodSync } from "node:fs";

// 延迟导入 postinstall 模块（需要在 mock 之后）
// 使用动态导入确保 mock 已生效

// ============================================
// 辅助工具
// ============================================

/** 模拟 installFromRelease 的类型 */
const mockInstallFromRelease = vi.mocked(installFromRelease);
const mockExistsSync = vi.mocked(existsSync);
const mockReaddirSync = vi.mocked(readdirSync);
const mockCopyFileSync = vi.mocked(copyFileSync);
const mockMkdirSync = vi.mocked(mkdirSync);
const mockChmodSync = vi.mocked(chmodSync);

// ============================================
// 测试套件
// ============================================

describe("postinstall 脚本", () => {
  // 保存原始环境变量
  const originalEnv = { ...process.env };

  beforeEach(() => {
    // 重置所有 mock
    vi.clearAllMocks();
    // 重置环境变量
    process.env = { ...originalEnv };
    // 清除 CI 相关环境变量
    delete process.env["CI"];
    delete process.env["GITHUB_ACTIONS"];
    delete process.env["JENKINS_URL"];
    delete process.env["GITLAB_CI"];
  });

  afterEach(() => {
    // 恢复原始环境变量
    process.env = originalEnv;
  });

  // ============================================
  // 测试 1: CI 环境自动跳过下载
  // ============================================
  describe("CI 环境检测", () => {
    it("CI=true 时跳过下载，输出提示信息", async () => {
      process.env["CI"] = "true";

      // 捕获控制台输出
      const consoleLogSpy = vi.spyOn(console, "log").mockImplementation(() => {});

      const { runPostinstall } = await import("../postinstall.js");
      await runPostinstall();

      // 验证没有调用 installFromRelease
      expect(mockInstallFromRelease).not.toHaveBeenCalled();

      // 验证输出包含 CI 环境跳过的提示
      const allOutput = consoleLogSpy.mock.calls.flat().join(" ");
      expect(allOutput).toContain("CI");

      consoleLogSpy.mockRestore();
    });

    it("GITHUB_ACTIONS=true 时同样跳过下载", async () => {
      process.env["GITHUB_ACTIONS"] = "true";

      const { runPostinstall } = await import("../postinstall.js");
      await runPostinstall();

      expect(mockInstallFromRelease).not.toHaveBeenCalled();
    });
  });

  // ============================================
  // 测试 2: 正常环境调用 installFromRelease
  // ============================================
  describe("正常安装流程", () => {
    it("非 CI 环境下调用 installFromRelease", async () => {
      mockInstallFromRelease.mockResolvedValue(undefined);

      const { runPostinstall } = await import("../postinstall.js");
      await runPostinstall();

      // 验证 installFromRelease 被调用
      expect(mockInstallFromRelease).toHaveBeenCalledOnce();
    });
  });

  // ============================================
  // 测试 3: 下载失败时回退到包内 bin/
  // ============================================
  describe("离线回退机制", () => {
    it("下载失败时回退到包内 bin/ 目录", async () => {
      // mock 网络下载失败
      mockInstallFromRelease.mockRejectedValue(new Error("网络连接失败: ECONNREFUSED"));

      // mock 包内 bin/ 目录存在对应二进制
      mockExistsSync.mockImplementation((p: unknown) => {
        const pathStr = String(p);
        // bin/ 目录下的二进制文件存在
        if (pathStr.includes("bin") && pathStr.includes("ghostcoded")) {
          return true;
        }
        return false;
      });

      // mock readdirSync 返回包内二进制列表
      mockReaddirSync.mockReturnValue(["ghostcoded-darwin-arm64"] as unknown as ReturnType<typeof readdirSync>);

      // mock copyFileSync 和 chmodSync 成功
      mockCopyFileSync.mockImplementation(() => {});
      mockMkdirSync.mockImplementation(() => undefined);
      mockChmodSync.mockImplementation(() => {});

      const consoleLogSpy = vi.spyOn(console, "log").mockImplementation(() => {});
      const consoleWarnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});

      const { runPostinstall } = await import("../postinstall.js");
      await runPostinstall();

      // 验证输出包含回退提示
      const allOutput = [
        ...consoleLogSpy.mock.calls.flat(),
        ...consoleWarnSpy.mock.calls.flat(),
      ].join(" ");
      expect(allOutput).toContain("回退");

      consoleLogSpy.mockRestore();
      consoleWarnSpy.mockRestore();
    });
  });

  // ============================================
  // 测试 4: 下载失败且包内 bin/ 不存在时报错
  // ============================================
  describe("双重失败处理", () => {
    it("下载失败且包内 bin/ 为空时输出明确错误信息", async () => {
      // mock 网络下载失败
      mockInstallFromRelease.mockRejectedValue(new Error("网络连接失败"));

      // mock 包内 bin/ 目录为空
      mockExistsSync.mockReturnValue(false);
      mockReaddirSync.mockReturnValue([] as unknown as ReturnType<typeof readdirSync>);

      const consoleErrorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      const { runPostinstall } = await import("../postinstall.js");
      await runPostinstall();

      // 验证输出明确的错误信息（包含安装建议）
      const errorOutput = consoleErrorSpy.mock.calls.flat().join(" ");
      expect(errorOutput.length).toBeGreaterThan(0);

      consoleErrorSpy.mockRestore();
    });
  });

  // ============================================
  // 测试 5: 权限不足时输出友好提示
  // ============================================
  describe("权限错误处理", () => {
    it("权限不足时输出包含修复建议的提示", async () => {
      // mock 权限不足错误（EACCES）
      const permError = Object.assign(new Error("权限不足"), { code: "EACCES" });
      mockInstallFromRelease.mockRejectedValue(permError);

      // mock 包内 bin/ 也无法访问（权限问题）
      mockExistsSync.mockImplementation(() => {
        throw permError;
      });

      const consoleErrorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
      const consoleLogSpy = vi.spyOn(console, "log").mockImplementation(() => {});

      const { runPostinstall } = await import("../postinstall.js");
      await runPostinstall();

      // 验证输出包含权限相关的修复建议
      const allOutput = [
        ...consoleErrorSpy.mock.calls.flat(),
        ...consoleLogSpy.mock.calls.flat(),
      ].join(" ");

      // 应该包含权限相关提示（chmod 或 sudo 或 权限）
      const hasPermissionHint =
        allOutput.includes("chmod") ||
        allOutput.includes("sudo") ||
        allOutput.includes("权限");
      expect(hasPermissionHint).toBe(true);

      consoleErrorSpy.mockRestore();
      consoleLogSpy.mockRestore();
    });
  });
});

// ============================================
// isCIEnvironment 单元测试
// ============================================
describe("isCIEnvironment 函数", () => {
  const originalEnv = { ...process.env };

  beforeEach(() => {
    process.env = { ...originalEnv };
    delete process.env["CI"];
    delete process.env["GITHUB_ACTIONS"];
    delete process.env["JENKINS_URL"];
    delete process.env["GITLAB_CI"];
  });

  afterEach(() => {
    process.env = originalEnv;
  });

  it("CI=true 时返回 true", async () => {
    process.env["CI"] = "true";
    const { isCIEnvironment } = await import("../postinstall.js");
    expect(isCIEnvironment()).toBe(true);
  });

  it("GITHUB_ACTIONS=true 时返回 true", async () => {
    process.env["GITHUB_ACTIONS"] = "true";
    const { isCIEnvironment } = await import("../postinstall.js");
    expect(isCIEnvironment()).toBe(true);
  });

  it("JENKINS_URL 存在时返回 true", async () => {
    process.env["JENKINS_URL"] = "http://jenkins.example.com";
    const { isCIEnvironment } = await import("../postinstall.js");
    expect(isCIEnvironment()).toBe(true);
  });

  it("GITLAB_CI=true 时返回 true", async () => {
    process.env["GITLAB_CI"] = "true";
    const { isCIEnvironment } = await import("../postinstall.js");
    expect(isCIEnvironment()).toBe(true);
  });

  it("无 CI 环境变量时返回 false", async () => {
    const { isCIEnvironment } = await import("../postinstall.js");
    expect(isCIEnvironment()).toBe(false);
  });
});
