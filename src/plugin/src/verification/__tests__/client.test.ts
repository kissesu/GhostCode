/**
 * @file client.test.ts
 * @description 验证客户端单元测试
 *              mock callDaemon 验证 IPC 调用链路
 *              TDD Red 阶段：测试先于实现
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { vi, describe, it, expect, beforeEach } from "vitest";

// ============================================
// Mock 声明（必须在顶层，vitest 静态分析需要）
// ============================================

vi.mock("../../ipc.js", () => ({
  callDaemon: vi.fn(),
}));

import { callDaemon } from "../../ipc.js";
import {
  startVerification,
  getVerificationStatus,
  cancelVerification,
  VerificationError,
} from "../client.js";
import type { RunState } from "../types.js";

// ============================================
// 测试辅助：构造 RunState 快照
// ============================================

/**
 * 构建测试用 RunState 快照
 * 模拟 Rust 侧 verification.rs 序列化输出
 */
function mockRunState(overrides: Partial<RunState> = {}): RunState {
  return {
    run_id: "run-001",
    group_id: "group-test",
    status: "Running",
    iteration: 0,
    max_iterations: 10,
    current_checks: [
      ["Build", "Pending"],
      ["Test", "Pending"],
      ["Lint", "Pending"],
    ],
    history: [],
    ...overrides,
  };
}

/**
 * 构建成功的 DaemonResponse
 */
function mockSuccessResponse(result: unknown) {
  return { v: 1 as const, ok: true, result };
}

/**
 * 构建失败的 DaemonResponse
 */
function mockErrorResponse(code: string, message: string) {
  return { v: 1 as const, ok: false, result: null, error: { code, message } };
}

// ============================================
// 测试套件
// ============================================

describe("验证客户端 - startVerification", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("调用 callDaemon('verification_start') 并返回 RunState", async () => {
    // Arrange: mock 返回 Running 状态的 RunState
    const expectedState = mockRunState();
    vi.mocked(callDaemon).mockResolvedValueOnce(
      mockSuccessResponse(expectedState)
    );

    // Act
    const result = await startVerification("group-test", "run-001");

    // Assert: 验证 callDaemon 被以正确参数调用
    expect(callDaemon).toHaveBeenCalledWith("verification_start", {
      group_id: "group-test",
      run_id: "run-001",
    });

    // Assert: 返回值与 mock RunState 一致
    expect(result).toEqual(expectedState);
    expect(result.run_id).toBe("run-001");
    expect(result.status).toBe("Running");
  });

  it("处理 Daemon 返回 ok: false 时抛出 VerificationError", async () => {
    // Arrange: mock 返回错误响应（注册两次，两个断言各消耗一次）
    vi.mocked(callDaemon)
      .mockResolvedValueOnce(
        mockErrorResponse("ALREADY_EXISTS", "运行 ID 已存在，无法重复启动")
      )
      .mockResolvedValueOnce(
        mockErrorResponse("ALREADY_EXISTS", "运行 ID 已存在，无法重复启动")
      );

    // Act & Assert: 验证抛出 VerificationError 类型
    await expect(
      startVerification("group-test", "run-001")
    ).rejects.toThrow(VerificationError);

    // Act & Assert: 验证错误属性正确
    await expect(
      startVerification("group-test", "run-001")
    ).rejects.toMatchObject({
      name: "VerificationError",
      code: "ALREADY_EXISTS",
      message: "运行 ID 已存在，无法重复启动",
    });
  });
});

describe("验证客户端 - getVerificationStatus", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("返回 Running 状态的 RunState", async () => {
    // Arrange
    const runningState = mockRunState({ status: "Running", iteration: 2 });
    vi.mocked(callDaemon).mockResolvedValueOnce(
      mockSuccessResponse(runningState)
    );

    // Act
    const result = await getVerificationStatus("group-test", "run-001");

    // Assert
    expect(callDaemon).toHaveBeenCalledWith("verification_status", {
      group_id: "group-test",
      run_id: "run-001",
    });
    expect(result.status).toBe("Running");
    expect(result.iteration).toBe(2);
  });

  it("处理 NOT_FOUND 错误时抛出 VerificationError", async () => {
    // Arrange: 模拟运行 ID 不存在
    vi.mocked(callDaemon).mockResolvedValueOnce(
      mockErrorResponse("NOT_FOUND", "运行 ID 不存在")
    );

    // Act & Assert
    await expect(
      getVerificationStatus("group-test", "nonexistent-run")
    ).rejects.toMatchObject({
      name: "VerificationError",
      code: "NOT_FOUND",
      message: "运行 ID 不存在",
    });
  });
});

describe("验证客户端 - cancelVerification", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("调用 callDaemon('verification_cancel') 取消运行", async () => {
    // Arrange
    vi.mocked(callDaemon).mockResolvedValueOnce(
      mockSuccessResponse({ cancelled: true })
    );

    // Act: 正常取消，不应抛出
    await expect(
      cancelVerification("group-test", "run-001")
    ).resolves.toBeUndefined();

    // Assert: 验证调用参数正确
    expect(callDaemon).toHaveBeenCalledWith("verification_cancel", {
      group_id: "group-test",
      run_id: "run-001",
    });
  });

  it("取消失败时抛出 VerificationError", async () => {
    // Arrange: 模拟运行已完成无法取消
    vi.mocked(callDaemon).mockResolvedValueOnce(
      mockErrorResponse("INVALID_STATE", "运行已完成，无法取消")
    );

    // Act & Assert
    await expect(
      cancelVerification("group-test", "run-001")
    ).rejects.toMatchObject({
      name: "VerificationError",
      code: "INVALID_STATE",
    });
  });
});
