/**
 * @file session-start.test.ts
 * @description SessionStart Hook 脚本单元测试（TDD）
 *              测试 hook-session-start.mjs 脚本的以下性质：
 *              1. 脚本文件存在且路径正确
 *              2. 脚本输出包含版本号和 skill 数量的格式正确初始化消息
 *              3. 幂等性：重复调用不重复初始化（状态文件已存在时跳过写入）
 * @author Atlas.oi
 * @date 2026-03-05
 */

import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execSync } from "node:child_process";
import { tmpdir } from "node:os";

// ============================================
// 路径常量
// ============================================

// 当前测试文件所在目录
const __dirname = dirname(fileURLToPath(import.meta.url));

// plugin 根目录（从 __tests__ 向上 3 级: __tests__ -> hooks -> src -> plugin）
const PLUGIN_ROOT = join(__dirname, "..", "..", "..");

// 目标脚本路径
const SCRIPT_PATH = join(PLUGIN_ROOT, "scripts", "hook-session-start.mjs");

// ============================================
// 辅助函数
// ============================================

/**
 * 创建临时 GHOSTCODE_HOME 目录，用于隔离测试环境
 *
 * @returns {{ dir: string, cleanup: () => void }} 临时目录路径和清理函数
 */
function createTempGhostcodeHome() {
  const dir = join(tmpdir(), `ghostcode-test-session-start-${Date.now()}-${Math.random().toString(36).slice(2)}`);
  mkdirSync(dir, { recursive: true });
  return {
    dir,
    cleanup: () => {
      try {
        rmSync(dir, { recursive: true, force: true });
      } catch {
        // 清理失败不影响测试
      }
    },
  };
}

/**
 * 执行脚本并捕获输出
 *
 * 业务逻辑：
 * 1. 通过 node 运行目标脚本
 * 2. 注入临时 GHOSTCODE_HOME 环境变量隔离状态文件
 * 3. 返回标准输出字符串
 *
 * @param {string} ghostcodeHome - 临时 GHOSTCODE_HOME 路径
 * @returns {{ stdout: string, stderr: string, exitCode: number }}
 */
function runScript(ghostcodeHome: string): { stdout: string; stderr: string; exitCode: number } {
  try {
    const stdout = execSync(`node "${SCRIPT_PATH}"`, {
      env: {
        ...process.env,
        GHOSTCODE_HOME: ghostcodeHome,
        // 注入假 CLAUDE_PLUGIN_ROOT，指向 plugin 根目录
        CLAUDE_PLUGIN_ROOT: PLUGIN_ROOT,
      },
      encoding: "utf-8",
      timeout: 5000,
      stdio: ["pipe", "pipe", "pipe"],
    });
    return { stdout, stderr: "", exitCode: 0 };
  } catch (error: any) {
    return {
      stdout: error.stdout || "",
      stderr: error.stderr || "",
      exitCode: error.status ?? 1,
    };
  }
}

// ============================================
// 测试套件 1：脚本文件存在性
// ============================================

describe("hook-session-start.mjs - 文件存在性", () => {
  it("脚本文件应存在于 scripts/ 目录下", () => {
    // 直接验证文件路径存在
    // Red 阶段：文件不存在时此测试应失败
    expect(existsSync(SCRIPT_PATH)).toBe(true);
  });

  it("脚本文件路径应为 scripts/hook-session-start.mjs", () => {
    // 验证路径后缀，确保文件名和位置正确
    expect(SCRIPT_PATH).toMatch(/scripts\/hook-session-start\.mjs$/);
  });
});

// ============================================
// 测试套件 2：初始化消息格式
// ============================================

describe("hook-session-start.mjs - 初始化消息格式", () => {
  let tempHome: { dir: string; cleanup: () => void };

  beforeEach(() => {
    // 为每个测试创建独立的临时目录，确保测试隔离
    tempHome = createTempGhostcodeHome();
  });

  afterEach(() => {
    tempHome.cleanup();
  });

  it("输出应包含 [GhostCode] 前缀", () => {
    const { stdout } = runScript(tempHome.dir);
    expect(stdout).toMatch(/\[GhostCode\]/);
  });

  it("输出应包含版本号（格式 vX.Y.Z）", () => {
    const { stdout } = runScript(tempHome.dir);
    // 匹配语义版本号格式
    expect(stdout).toMatch(/v\d+\.\d+\.\d+/);
  });

  it("输出应包含 skill 数量信息", () => {
    const { stdout } = runScript(tempHome.dir);
    // 应包含 skills loaded 字样
    expect(stdout).toMatch(/skill/i);
  });

  it("输出格式应匹配：[GhostCode] Plugin vX.Y.Z | N skills loaded | Daemon: pending", () => {
    const { stdout } = runScript(tempHome.dir);
    // 完整格式匹配
    expect(stdout).toMatch(/\[GhostCode\] Plugin v\d+\.\d+\.\d+ \| \d+ skills loaded \| Daemon: \w+/);
  });

  it("脚本应以退出码 0 正常退出", () => {
    const { exitCode } = runScript(tempHome.dir);
    expect(exitCode).toBe(0);
  });
});

// ============================================
// 测试套件 3：幂等性验证
// ============================================

describe("hook-session-start.mjs - 幂等性", () => {
  let tempHome: { dir: string; cleanup: () => void };

  beforeEach(() => {
    tempHome = createTempGhostcodeHome();
  });

  afterEach(() => {
    tempHome.cleanup();
  });

  it("重复调用两次不应报错，均正常退出", () => {
    // 第一次调用
    const first = runScript(tempHome.dir);
    expect(first.exitCode).toBe(0);

    // 第二次调用（状态文件已存在）
    const second = runScript(tempHome.dir);
    expect(second.exitCode).toBe(0);
  });

  it("状态文件存在时，初始化消息仍应输出（用于 SessionStart 事件通知）", () => {
    // 首次调用：创建状态文件
    runScript(tempHome.dir);

    // 再次调用：输出仍应包含初始化消息
    const { stdout } = runScript(tempHome.dir);
    expect(stdout).toMatch(/\[GhostCode\]/);
  });

  it("状态文件已存在时，状态文件不应被覆盖（幂等保护）", () => {
    // 预创建状态文件，写入特定内容
    const stateDir = join(tempHome.dir, "state");
    mkdirSync(stateDir, { recursive: true });
    const stateFile = join(stateDir, "hook-state.json");
    const originalState = JSON.stringify({ daemonStarted: true, socketPath: "/test/socket", leaseId: "test-lease" });
    writeFileSync(stateFile, originalState, "utf-8");

    // 调用脚本
    runScript(tempHome.dir);

    // 验证状态文件内容保持不变
    const { readFileSync } = require("node:fs");
    const afterState = readFileSync(stateFile, "utf-8");
    expect(afterState).toBe(originalState);
  });
});
