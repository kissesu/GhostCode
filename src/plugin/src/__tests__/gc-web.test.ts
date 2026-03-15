/**
 * @file gc-web.test.ts
 * @description /gc-web Magic Keyword Handler 的单元测试
 *              验证 Dashboard URL 构建、浏览器启动、Token 携带等逻辑
 *              handleGcWeb 的集成测试需要 mock ensureWeb()，此处聚焦纯函数测试
 * @author Atlas.oi
 * @date 2026-03-15
 */

import { describe, it, expect, vi, beforeEach } from "vitest";
import { getDashboardUrl } from "../keywords/gc-web.js";

// ============================================
// 测试套件：getDashboardUrl - Dashboard URL 构建
// ============================================
describe("getDashboardUrl", () => {
  // ============================================
  // 测试用例 1：解析 dashboard URL 包含正确端口
  // ============================================
  it("解析 dashboard URL 包含正确端口", () => {
    const url = getDashboardUrl(7070);
    // 验证返回 http://127.0.0.1:7070（与 ghostcode-web 默认 bind 地址一致）
    expect(url).toBe("http://127.0.0.1:7070");
  });

  // ============================================
  // 测试用例 2：自动携带认证 Token
  // ============================================
  it("自动携带认证 Token", () => {
    const url = getDashboardUrl(7070, "test-token-abc123");
    expect(url).toContain("token=test-token-abc123");
  });

  it("无 Token 时不添加查询参数", () => {
    const url = getDashboardUrl(8080);
    expect(url).toBe("http://127.0.0.1:8080");
  });

  it("Token 包含特殊字符时正确编码", () => {
    const url = getDashboardUrl(7070, "token with spaces&special=chars");
    expect(url).toContain("token=token%20with%20spaces%26special%3Dchars");
  });

  it("空 Token 字符串不添加查询参数", () => {
    const url = getDashboardUrl(7070, "");
    expect(url).toBe("http://127.0.0.1:7070");
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
  // 测试用例：在 macOS 上调用 open 命令
  // ============================================
  it("在 macOS 上调用 open 命令", async () => {
    const originalPlatform = process.platform;
    Object.defineProperty(process, "platform", {
      value: "darwin",
      writable: true,
      configurable: true,
    });

    const { execFile } = await import("child_process");
    const mockExec = vi.fn((_cmd, _args, cb) => {
      cb(null, "", "");
      return {} as ReturnType<typeof execFile>;
    });

    const { openURLWithExec } = await import("../utils/browser.js");
    await openURLWithExec("http://127.0.0.1:7070", mockExec as Parameters<typeof openURLWithExec>[1]);

    expect(mockExec).toHaveBeenCalledWith(
      "open",
      ["http://127.0.0.1:7070"],
      expect.any(Function)
    );

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
    await openURLWithExec("http://127.0.0.1:7070", mockExec as Parameters<typeof openURLWithExec>[1]);

    expect(mockExec).toHaveBeenCalledWith(
      "xdg-open",
      ["http://127.0.0.1:7070"],
      expect.any(Function)
    );

    Object.defineProperty(process, "platform", {
      value: originalPlatform,
      writable: true,
      configurable: true,
    });
  });
});
