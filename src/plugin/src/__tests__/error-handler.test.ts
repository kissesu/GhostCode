/**
 * @file 错误处理模块测试
 * @description 测试 error-handler 工具函数的核心行为：
 *              1. 将内部错误转化为用户可读的结构化错误信息
 *              2. 每个错误包含 Code + Suggestion
 *              3. 未知错误被包装并保留原始信息
 *              4. 特定错误包含 ghostcode 修复命令提示
 *              5. Markdown 格式化输出
 * @author Atlas.oi
 * @date 2026-03-04
 */

import { describe, it, expect } from "vitest";
import {
  formatErrorWithFix,
  formatErrorAsMarkdown,
  type UserFacingError,
} from "../utils/error-handler";

// ============================================
// 测试套件：错误格式化核心行为
// ============================================

describe("formatErrorWithFix", () => {
  // 测试用例 1: 格式化 IPC 连接错误为人类可读建议
  it("格式化 IPC 连接错误为人类可读建议", () => {
    // ECONNREFUSED 是典型的 Daemon 未启动错误
    const error = new Error("connect ECONNREFUSED /tmp/ghostcode.sock");
    error.message = "connect ECONNREFUSED /tmp/ghostcode.sock";
    (error as NodeJS.ErrnoException).code = "ECONNREFUSED";

    const result = formatErrorWithFix(error);

    // 错误码必须是 GC_IPC_001
    expect(result.code).toBe("GC_IPC_001");
    // 必须有描述
    expect(result.description).toBeTruthy();
    // 必须有修复建议
    expect(result.suggestion).toBeTruthy();
  });

  // 测试用例 2: 每个错误包含 Code 和 Suggestion
  it("每个预定义错误类型都有 code 和 suggestion 字段", () => {
    // 测试几种典型错误类型
    const testCases: Array<{ code: string; expectedPrefix: string }> = [
      { code: "ECONNREFUSED", expectedPrefix: "GC_IPC_" },
      { code: "CONFIG_NOT_FOUND", expectedPrefix: "GC_CONFIG_" },
      { code: "BINARY_NOT_FOUND", expectedPrefix: "GC_BINARY_" },
      { code: "DAEMON_CRASHED", expectedPrefix: "GC_RUNTIME_" },
    ];

    for (const { code, expectedPrefix } of testCases) {
      const error = new Error(code);
      (error as NodeJS.ErrnoException).code = code;

      const result = formatErrorWithFix(error);

      // 错误码格式必须符合 GC_{CATEGORY}_{NNN}
      expect(result.code).toMatch(/^GC_[A-Z]+_\d{3}$/);
      // 错误码前缀必须匹配预期分类
      expect(result.code).toContain(expectedPrefix);
      // suggestion 字段必须存在且非空
      expect(result.suggestion).toBeTruthy();
      expect(typeof result.suggestion).toBe("string");
    }
  });

  // 测试用例 3: 格式化未知错误保留原始信息
  it("格式化未知错误保留原始信息并包装为 UserFacingError", () => {
    const originalMessage = "some_totally_unknown_error_xyz_999";
    const error = new Error(originalMessage);

    const result = formatErrorWithFix(error);

    // 未知错误 code 为 GC_UNKNOWN_000
    expect(result.code).toBe("GC_UNKNOWN_000");
    // 原始错误信息必须被保留
    expect(result.originalError).toBeDefined();
    expect(result.originalError?.message).toBe(originalMessage);
    // 必须有 suggestion（即使是通用的）
    expect(result.suggestion).toBeTruthy();
  });

  // 测试用例 4: ghostcode 修复命令提示
  it("特定错误类型的 suggestion 中包含 ghostcode 修复命令", () => {
    // ECONNREFUSED 应提示 ghostcode doctor
    const ipcError = new Error("ECONNREFUSED");
    (ipcError as NodeJS.ErrnoException).code = "ECONNREFUSED";
    const ipcResult = formatErrorWithFix(ipcError);
    // fixCommand 或 suggestion 中包含 ghostcode doctor
    const ipcHasCommand =
      ipcResult.fixCommand?.includes("ghostcode doctor") ||
      ipcResult.suggestion?.includes("ghostcode doctor");
    expect(ipcHasCommand).toBe(true);

    // BINARY_NOT_FOUND 应提示 ghostcode init
    const binaryError = new Error("BINARY_NOT_FOUND");
    (binaryError as NodeJS.ErrnoException).code = "BINARY_NOT_FOUND";
    const binaryResult = formatErrorWithFix(binaryError);
    // fixCommand 或 suggestion 中包含 ghostcode init
    const binaryHasCommand =
      binaryResult.fixCommand?.includes("ghostcode init") ||
      binaryResult.suggestion?.includes("ghostcode init");
    expect(binaryHasCommand).toBe(true);
  });
});

// ============================================
// 测试套件：Markdown 格式化输出
// ============================================

describe("formatErrorAsMarkdown", () => {
  // 测试用例 5: Markdown 格式输出
  it("Markdown 格式输出符合规范（含代码块、粗体等）", () => {
    const userError: UserFacingError = {
      code: "GC_IPC_001",
      title: "Daemon 连接失败",
      description: "无法连接到 GhostCode Daemon",
      suggestion: "请确保 Daemon 正在运行",
      fixCommand: "ghostcode doctor",
    };

    const markdown = formatErrorAsMarkdown(userError);

    // 必须包含粗体标题（** 标记）
    expect(markdown).toMatch(/\*\*.+\*\*/);
    // 必须包含错误码
    expect(markdown).toContain("GC_IPC_001");
    // 如果有 fixCommand，必须出现在代码块中（` 或 ``` 标记）
    expect(markdown).toMatch(/`ghostcode doctor`|```[\s\S]*ghostcode doctor[\s\S]*```/);
    // 必须有换行（多行 Markdown）
    expect(markdown).toContain("\n");
  });
});
