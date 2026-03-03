/**
 * @file client.ts
 * @description 验证客户端 IPC 调用封装
 *              提供 TS 侧驱动 Daemon 验证生命周期的高级 API
 *              支持完整的 start → status → cancel 验证流程
 *              参考: oh-my-claudecode/src/skills/ralph.ts - Ralph 验证循环
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { callDaemon } from "../ipc.js";
import type { RunState } from "./types.js";

// ============================================
// 错误类型定义
// ============================================

/**
 * 验证客户端错误
 *
 * 封装 Daemon 返回的错误信息，提供结构化的错误码和消息。
 * 所有 Daemon 错误响应（ok: false）均通过此错误类型抛出。
 */
export class VerificationError extends Error {
  /**
   * @param code - Daemon 返回的错误码，如 "NOT_FOUND"、"ALREADY_EXISTS"
   * @param message - 可读的错误描述
   */
  constructor(
    public readonly code: string,
    message: string
  ) {
    super(message);
    this.name = "VerificationError";
  }
}

// ============================================
// 内部辅助函数
// ============================================

/**
 * 从 Daemon 响应中提取 RunState，失败时抛出 VerificationError
 *
 * @param resp - Daemon 响应对象
 * @param defaultMessage - Daemon 无错误信息时的默认错误提示
 * @returns 解析后的 RunState
 * @throws {VerificationError} 当 resp.ok 为 false 时抛出
 */
function extractRunState(
  resp: { ok: boolean; result: unknown; error?: { code: string; message: string } },
  defaultMessage: string
): RunState {
  if (!resp.ok) {
    throw new VerificationError(
      resp.error?.code ?? "UNKNOWN",
      resp.error?.message ?? defaultMessage
    );
  }

  // 运行时类型守卫：校验 Daemon 返回结构的关键字段
  // 防止 IPC 协议不匹配时静默产生错误数据
  const result = resp.result as Record<string, unknown>;
  if (
    typeof result !== "object" ||
    result === null ||
    typeof result.run_id !== "string" ||
    typeof result.status !== "string"
  ) {
    throw new VerificationError(
      "INVALID_RESPONSE",
      "Daemon 返回的 RunState 结构不符合预期",
    );
  }

  return result as unknown as RunState;
}

// ============================================
// 公共 API
// ============================================

/**
 * 启动新的验证运行
 *
 * 业务逻辑说明：
 * 1. 向 Daemon 发送 verification_start 请求
 * 2. 传入 group_id 和 run_id 作为唯一标识
 * 3. Daemon 创建新的 RunState 并返回初始快照
 *
 * @param groupId - Agent 分组 ID
 * @param runId - 运行唯一标识符（由调用方生成）
 * @returns 初始化完成的 RunState 快照
 * @throws {VerificationError} 当 Daemon 返回错误时抛出（如 ALREADY_EXISTS）
 */
export async function startVerification(
  groupId: string,
  runId: string
): Promise<RunState> {
  const resp = await callDaemon("verification_start", {
    group_id: groupId,
    run_id: runId,
  });
  return extractRunState(resp, "验证启动失败");
}

/**
 * 查询验证运行状态
 *
 * 业务逻辑说明：
 * 1. 向 Daemon 发送 verification_status 请求
 * 2. 传入 group_id 和 run_id 定位目标运行
 * 3. Daemon 返回当前 RunState 快照（包含实时检查状态）
 *
 * @param groupId - Agent 分组 ID
 * @param runId - 运行唯一标识符
 * @returns 当前 RunState 快照
 * @throws {VerificationError} 当运行不存在（NOT_FOUND）或其他错误时抛出
 */
export async function getVerificationStatus(
  groupId: string,
  runId: string
): Promise<RunState> {
  const resp = await callDaemon("verification_status", {
    group_id: groupId,
    run_id: runId,
  });
  return extractRunState(resp, "验证状态查询失败");
}

/**
 * 取消验证运行
 *
 * 业务逻辑说明：
 * 1. 向 Daemon 发送 verification_cancel 请求
 * 2. 传入 group_id 和 run_id 定位目标运行
 * 3. Daemon 将运行状态设置为 Cancelled
 * 4. 成功时静默返回（不返回值）
 *
 * @param groupId - Agent 分组 ID
 * @param runId - 运行唯一标识符
 * @throws {VerificationError} 当运行不存在或状态不允许取消时抛出
 */
export async function cancelVerification(
  groupId: string,
  runId: string
): Promise<void> {
  const resp = await callDaemon("verification_cancel", {
    group_id: groupId,
    run_id: runId,
  });
  if (!resp.ok) {
    throw new VerificationError(
      resp.error?.code ?? "UNKNOWN",
      resp.error?.message ?? "验证取消失败"
    );
  }
}
