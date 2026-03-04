/**
 * @file cli-init.test.ts
 * @description ghostcode init 子命令的单元测试
 *   TDD Red 阶段：先写测试，验证测试失败，再实现
 * @author Atlas.oi
 * @date 2026-03-04
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { homedir } from "node:os";
import { join } from "node:path";

// ============================================
// 测试常量
// ============================================

const GHOSTCODE_HOME = join(homedir(), ".ghostcode");
const GHOSTCODE_BIN_DIR = join(GHOSTCODE_HOME, "bin");

// ============================================
// 测试套件
// ============================================

describe("runInitCommand", () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("首次执行创建 ~/.ghostcode/ 基础目录", async () => {
    // mock fs 模块：目录不存在
    const mkdirSyncMock = vi.fn();
    const existsSyncMock = vi.fn().mockReturnValue(false);

    vi.doMock("node:fs", () => ({
      existsSync: existsSyncMock,
      mkdirSync: mkdirSyncMock,
      writeFileSync: vi.fn(),
      readFileSync: vi.fn(),
    }));

    // mock installFromRelease：成功安装
    vi.doMock("../install.js", () => ({
      installFromRelease: vi.fn().mockResolvedValue(undefined),
    }));

    // 动态导入（在 mock 之后）
    const { runInitCommand } = await import("../cli/init.js");

    const result = await runInitCommand({ dryRun: true });

    // 验证结果表明目录创建被触发
    expect(result.dirsCreated).toBe(true);
  });

  it("重复执行幂等，不覆盖用户配置", async () => {
    vi.resetModules();

    // mock fs：目录和配置均已存在
    const existsSyncMock = vi.fn().mockReturnValue(true);
    const writeFileSyncMock = vi.fn();

    vi.doMock("node:fs", () => ({
      existsSync: existsSyncMock,
      mkdirSync: vi.fn(),
      writeFileSync: writeFileSyncMock,
      readFileSync: vi.fn().mockReturnValue(
        JSON.stringify({
          mcpServers: {
            ghostcode: { command: join(GHOSTCODE_BIN_DIR, "ghostcode-mcp") },
          },
        })
      ),
    }));

    vi.doMock("../install.js", () => ({
      installFromRelease: vi.fn().mockResolvedValue(undefined),
    }));

    const { runInitCommand } = await import("../cli/init.js");

    // 应该幂等，不抛异常
    const result = await runInitCommand();
    expect(result.success).toBe(true);
  });

  it("未检测到二进制时触发安装流程", async () => {
    vi.resetModules();

    // mock fs：目录存在但 bin 为空（二进制不存在）
    const existsSyncMock = vi.fn().mockImplementation((p: string) => {
      // bin 目录本身存在，但 ghostcoded 和 ghostcode-mcp 不存在
      if (p === GHOSTCODE_HOME || p === GHOSTCODE_BIN_DIR) {
        return true;
      }
      return false;
    });

    vi.doMock("node:fs", () => ({
      existsSync: existsSyncMock,
      mkdirSync: vi.fn(),
      writeFileSync: vi.fn(),
      readFileSync: vi.fn().mockReturnValue("{}"),
    }));

    // mock installFromRelease 为成功
    const installFromReleaseMock = vi.fn().mockResolvedValue(undefined);
    vi.doMock("../install.js", () => ({
      installFromRelease: installFromReleaseMock,
    }));

    const { runInitCommand } = await import("../cli/init.js");

    await runInitCommand();

    // 验证 installFromRelease 被调用
    expect(installFromReleaseMock).toHaveBeenCalled();
  });
});
