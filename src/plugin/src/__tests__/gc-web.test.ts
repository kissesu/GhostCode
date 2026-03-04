/**
 * @file gc-web.test.ts
 * @description /gc-web Magic Keyword Handler 的单元测试
 * 验证 Dashboard URL 构建、浏览器启动、Token 携带、错误处理等逻辑
 * @author Atlas.oi
 * @date 2026-03-04
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { getDashboardUrl, handleGcWeb } from "../keywords/gc-web.js";
import * as browserModule from "../utils/browser.js";

// ============================================
// 测试套件：getDashboardUrl - Dashboard URL 构建
// ============================================
describe("getDashboardUrl", () => {
  // ============================================
  // 测试用例 1：解析 dashboard URL 包含正确端口
  // ============================================
  it("解析 dashboard URL 包含正确端口", () => {
    // 调用 getDashboardUrl，传入端口 3000
    const url = getDashboardUrl(3000);
    // 验证返回 http://localhost:3000
    expect(url).toBe("http://localhost:3000");
  });

  // ============================================
  // 测试用例 3：自动携带认证 Token
  // ============================================
  it("自动携带认证 Token", () => {
    // mock session token 存在，调用 getDashboardUrl
    const url = getDashboardUrl(3000, "test-token-abc123");
    // 验证 URL 包含 token 参数
    expect(url).toContain("token=test-token-abc123");
  });

  it("无 Token 时不添加查询参数", () => {
    const url = getDashboardUrl(8080);
    // 不含查询参数
    expect(url).toBe("http://localhost:8080");
  });
});

// ============================================
// 测试套件：handleGcWeb - 处理 /gc-web 关键词
// ============================================
describe("handleGcWeb", () => {
  beforeEach(() => {
    // 重置所有 mock 状态
    vi.restoreAllMocks();
  });

  // ============================================
  // 测试用例 2：dashboard 未运行时抛出错误
  // ============================================
  it("dashboard 未运行时抛出错误", async () => {
    // mock checkRunning 返回 false（dashboard 未运行）
    const checkRunning = vi.fn().mockResolvedValue(false);

    // 验证抛出包含 "Dashboard 未运行" 的错误
    await expect(
      handleGcWeb({ port: 3000, checkRunning })
    ).rejects.toThrow("Dashboard 未运行");
  });

  it("dashboard 正在运行时成功打开浏览器", async () => {
    // mock checkRunning 返回 true
    const checkRunning = vi.fn().mockResolvedValue(true);
    // mock openURL，避免真实调用浏览器
    const mockOpenURL = vi.fn().mockResolvedValue(undefined);
    vi.spyOn(browserModule, "openURL").mockImplementation(mockOpenURL);

    const result = await handleGcWeb({ port: 3000, checkRunning });

    // 验证调用了 openURL
    expect(mockOpenURL).toHaveBeenCalledOnce();
    // 验证返回的 URL 包含端口
    expect(result).toContain("localhost:3000");
  });

  it("携带 Token 时 URL 包含 token 参数", async () => {
    const checkRunning = vi.fn().mockResolvedValue(true);
    const mockOpenURL = vi.fn().mockResolvedValue(undefined);
    vi.spyOn(browserModule, "openURL").mockImplementation(mockOpenURL);

    const result = await handleGcWeb({
      port: 3000,
      token: "my-session-token",
      checkRunning,
    });

    // 验证 URL 包含 token 参数
    expect(result).toContain("token=my-session-token");
    // 验证 openURL 也用了带 Token 的 URL
    expect(mockOpenURL).toHaveBeenCalledWith(
      expect.stringContaining("token=my-session-token")
    );
  });
});

// ============================================
// 测试套件：openURL - 跨平台浏览器启动
// ============================================
describe("openURL 跨平台浏览器启动", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  // ============================================
  // 测试用例 4：跨平台浏览器启动调用正确命令
  // ============================================
  it("在 macOS 上调用 open 命令", async () => {
    const { openURL } = await import("../utils/browser.js");

    // mock process.platform 为 darwin
    const originalPlatform = process.platform;
    Object.defineProperty(process, "platform", {
      value: "darwin",
      writable: true,
      configurable: true,
    });

    // mock child_process.exec 或 spawn，避免真实启动浏览器
    const { execFile } = await import("child_process");
    // 注意：直接测试模块行为比 mock 底层更符合意图
    // 通过注入 execFn 来测试跨平台逻辑
    const mockExec = vi.fn((_cmd, _args, cb) => {
      cb(null, "", "");
      return {} as ReturnType<typeof execFile>;
    });

    // 使用可注入版本进行测试
    const { openURLWithExec } = await import("../utils/browser.js");
    await openURLWithExec("http://localhost:3000", mockExec as Parameters<typeof openURLWithExec>[1]);

    // 验证在 darwin 上调用了 'open' 命令
    expect(mockExec).toHaveBeenCalledWith(
      "open",
      ["http://localhost:3000"],
      expect.any(Function)
    );

    // 恢复 platform
    Object.defineProperty(process, "platform", {
      value: originalPlatform,
      writable: true,
      configurable: true,
    });
  });

  it("在 Linux 上调用 xdg-open 命令", async () => {
    const originalPlatform = process.platform;
    Object.defineProperty(process, "platform", {
      value: "linux",
      writable: true,
      configurable: true,
    });

    const { execFile } = await import("child_process");
    const mockExec = vi.fn((_cmd, _args, cb) => {
      cb(null, "", "");
      return {} as ReturnType<typeof execFile>;
    });

    const { openURLWithExec } = await import("../utils/browser.js");
    await openURLWithExec("http://localhost:3000", mockExec as Parameters<typeof openURLWithExec>[1]);

    // 验证在 linux 上调用了 'xdg-open' 命令
    expect(mockExec).toHaveBeenCalledWith(
      "xdg-open",
      ["http://localhost:3000"],
      expect.any(Function)
    );

    Object.defineProperty(process, "platform", {
      value: originalPlatform,
      writable: true,
      configurable: true,
    });
  });
});
