import { RunState } from './types.js';

/**
 * @file client.ts
 * @description 验证客户端 IPC 调用封装
 *              提供 TS 侧驱动 Daemon 验证生命周期的高级 API
 *              支持完整的 start → status → cancel 验证流程
 *              参考: oh-my-claudecode/src/skills/ralph.ts - Ralph 验证循环
 * @author Atlas.oi
 * @date 2026-03-03
 */

/**
 * 验证客户端错误
 *
 * 封装 Daemon 返回的错误信息，提供结构化的错误码和消息。
 * 所有 Daemon 错误响应（ok: false）均通过此错误类型抛出。
 */
declare class VerificationError extends Error {
    readonly code: string;
    /**
     * @param code - Daemon 返回的错误码，如 "NOT_FOUND"、"ALREADY_EXISTS"
     * @param message - 可读的错误描述
     */
    constructor(code: string, message: string);
}
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
declare function startVerification(groupId: string, runId: string): Promise<RunState>;
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
declare function getVerificationStatus(groupId: string, runId: string): Promise<RunState>;
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
declare function cancelVerification(groupId: string, runId: string): Promise<void>;

export { VerificationError, cancelVerification, getVerificationStatus, startVerification };
