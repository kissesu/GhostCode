/**
 * @file state.test.ts
 * @description Magic Keywords 状态管理的单元测试
 * 验证状态文件读写、默认值、幂等性等行为
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as fs from "node:fs/promises";
import * as path from "node:path";
import * as os from "node:os";
import { readKeywordState, writeKeywordState } from "../state.js";
import type { KeywordState } from "../types.js";

// 使用临时目录隔离每个测试用例的文件系统操作
let tmpDir: string;

beforeEach(async () => {
  tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "ghostcode-test-"));
});

afterEach(async () => {
  await fs.rm(tmpDir, { recursive: true, force: true });
});

describe("writeKeywordState", () => {
  // ============================================
  // 测试 1：写入状态文件成功
  // ============================================
  it("写入状态文件成功", async () => {
    const state: KeywordState = {
      active: "ralph",
      activatedAt: "2026-03-03T10:00:00.000Z",
      prompt: "ralph 请分析代码",
    };

    await writeKeywordState(tmpDir, state);

    // 验证文件确实存在
    const statePath = path.join(tmpDir, ".ghostcode", "state", "keywords.json");
    const exists = await fs
      .access(statePath)
      .then(() => true)
      .catch(() => false);
    expect(exists).toBe(true);
  });

  it("写入时自动创建目录层级", async () => {
    const state: KeywordState = {
      active: "ultrawork",
      activatedAt: "2026-03-03T10:00:00.000Z",
      prompt: "ultrawork 开始",
    };

    // tmpDir 下不存在 .ghostcode/state 目录，应自动创建
    await expect(writeKeywordState(tmpDir, state)).resolves.not.toThrow();
  });
});

describe("readKeywordState", () => {
  // ============================================
  // 测试 2：读取状态文件成功
  // ============================================
  it("读取已写入的状态文件", async () => {
    const state: KeywordState = {
      active: "autopilot",
      activatedAt: "2026-03-03T11:00:00.000Z",
      prompt: "autopilot 运行",
    };

    await writeKeywordState(tmpDir, state);
    const read = await readKeywordState(tmpDir);

    expect(read.active).toBe("autopilot");
    expect(read.activatedAt).toBe("2026-03-03T11:00:00.000Z");
    expect(read.prompt).toBe("autopilot 运行");
  });

  // ============================================
  // 测试 3：文件不存在时返回默认状态
  // 默认状态：active=null, activatedAt=null, prompt=null
  // ============================================
  it("文件不存在时返回默认状态", async () => {
    const state = await readKeywordState(tmpDir);

    expect(state.active).toBeNull();
    expect(state.activatedAt).toBeNull();
    expect(state.prompt).toBeNull();
  });

  // ============================================
  // 测试 4：幂等写入（写两次结果相同）
  // ============================================
  it("幂等写入：两次写入相同内容，读取结果一致", async () => {
    const state: KeywordState = {
      active: "team",
      activatedAt: "2026-03-03T12:00:00.000Z",
      prompt: "team 协作",
    };

    await writeKeywordState(tmpDir, state);
    await writeKeywordState(tmpDir, state);

    const read = await readKeywordState(tmpDir);
    expect(read.active).toBe("team");
    expect(read.activatedAt).toBe("2026-03-03T12:00:00.000Z");
    expect(read.prompt).toBe("team 协作");
  });

  it("幂等写入：更新状态后读取最新值", async () => {
    const state1: KeywordState = {
      active: "ralph",
      activatedAt: "2026-03-03T12:00:00.000Z",
      prompt: "第一次",
    };
    const state2: KeywordState = {
      active: "ultrawork",
      activatedAt: "2026-03-03T13:00:00.000Z",
      prompt: "第二次",
    };

    await writeKeywordState(tmpDir, state1);
    await writeKeywordState(tmpDir, state2);

    const read = await readKeywordState(tmpDir);
    // 第二次写入覆盖第一次
    expect(read.active).toBe("ultrawork");
    expect(read.prompt).toBe("第二次");
  });
});
