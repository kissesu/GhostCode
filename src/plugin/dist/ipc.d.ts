import * as net from 'node:net';
import { StreamEvent, StreamCallbacks } from './router/streaming.js';

/**
 * @file IPC 桥接层
 * @description GhostCode Plugin 与 Rust Daemon 之间的 Unix socket IPC 通信层。
 *              协议：换行符分隔的 JSON（newline-delimited JSON），每条消息为一行 JSON + \n。
 *              连接策略：维护 1 个持久连接，断开后懒重连（下次请求时自动重连）。
 *              超时：单次请求 10s，重连最多 3 次，指数退避（100ms / 200ms / 400ms）。
 *
 *              类型设计与 Rust 侧 DaemonRequest/DaemonResponse 完全对齐：
 *              参考: crates/ghostcode-types/src/ipc.rs
 *              参考: schemas/daemon-request.json, schemas/daemon-response.json
 * @author Atlas.oi
 * @date 2026-03-02
 */

/** Daemon 请求结构 - 与 DaemonRequest Rust struct 字段一一对应 */
interface DaemonRequest {
    /** 协议版本，固定为整数 1（与 Rust v: u8 = 1 对应） */
    v: 1;
    /** 操作名称，如 "ping"、"actor.add" */
    op: string;
    /** 操作参数，任意 JSON 对象（空参数时传 {}，不传 null） */
    args: Record<string, unknown>;
}
/** Daemon 错误结构 - 与 DaemonError Rust struct 字段一一对应 */
interface DaemonError {
    /** 错误码，如 "NOT_FOUND"、"INVALID_ARGS" */
    code: string;
    /** 可读的错误描述 */
    message: string;
}
/** Daemon 响应结构 - 与 DaemonResponse Rust struct 字段一一对应 */
interface DaemonResponse {
    /** 协议版本，固定为整数 1 */
    v: 1;
    /** 操作是否成功 */
    ok: boolean;
    /** 成功时的返回值（对应 Rust serde_json::Value，可为任意 JSON） */
    result: unknown;
    /** 失败时的错误信息 */
    error?: DaemonError;
}
/** IPC 请求超时（单次请求超过 10s 未收到响应） */
declare class IpcTimeoutError extends Error {
    constructor(op: string, timeoutMs: number);
}
/** IPC 连接失败（无法建立或恢复 Unix socket 连接） */
declare class IpcConnectionError extends Error {
    constructor(socketPath: string, cause?: Error);
}
/** IPC 协议错误（收到无效 JSON 或响应结构不符合 Schema） */
declare class IpcProtocolError extends Error {
    constructor(detail: string);
}
/**
 * 创建到 Daemon Unix socket 的原始连接
 *
 * 参考: cccc/src/cccc/daemon/client_ops.py:40-55 - AF_UNIX connect 逻辑
 *
 * @param socketPath - Unix socket 文件路径
 * @returns 已建立连接的 net.Socket 实例
 * @throws {IpcConnectionError} 无法连接时抛出
 */
declare function createConnection(socketPath: string): Promise<net.Socket>;
/**
 * 流式请求的聚合响应
 * 收集流结束时所有已到达的事件和锁定的 sessionId
 */
interface StreamResponse {
    /** 流过程中收到的所有事件，按到达顺序排列 */
    events: StreamEvent[];
    /** 从首个携带 session_id 的事件中锁定的会话 ID，若流中无 session_id 则为 null */
    sessionId: string | null;
}
/**
 * 解析 Socket 路径（三级回退策略）
 *
 * 优先级规则：
 * 1. 显式参数 - 调用方直接传入的路径，优先级最高
 * 2. 环境变量 GHOSTCODE_SOCKET_PATH - Daemon 启动后由 preToolUseHandler 注入
 * 3. addr.json 文件 - 从 ~/.ghostcode/daemon/ghostcoded.addr.json 读取 path 字段
 * 4. 全部失败 - 返回 null，由调用方决定如何处理
 *
 * @param explicit - 显式指定的 socket 路径（可选）
 * @returns 解析到的 socket 路径，若三级均无法找到则返回 null
 */
declare function resolveSocketPath(explicit?: string): string | null;
/**
 * 向 Daemon 发起一次 RPC 调用（公共 API）
 *
 * 参考: cccc/src/cccc/daemon/server.py:968-988 - call_daemon() 函数设计
 *
 * @param op - 操作名称
 * @param args - 操作参数（默认为 {}）
 * @param socketPath - Unix socket 路径（可选，未传入时通过 resolveSocketPath 自动发现）
 * @returns Daemon 响应
 * @throws {IpcConnectionError} 无法解析 Socket 路径时抛出
 */
declare function callDaemon(op: string, args?: Record<string, unknown>, socketPath?: string): Promise<DaemonResponse>;
/**
 * 重置模块级单例（主要用于测试场景）
 */
declare function resetClient(): Promise<void>;
/**
 * 向 Daemon 发起流式 RPC 调用（公共 API）
 *
 * 在 callDaemon() 的基础上增加多行 JSON 流式支持。
 * 已有的 callDaemon() 接口不受影响，完全向后兼容。
 *
 * @param op - 操作名称，如 "task.route"
 * @param args - 操作参数（默认为 {}）
 * @param callbacks - 流式事件回调集合
 * @param socketPath - Unix socket 路径（可选，未传入时通过 resolveSocketPath 自动发现）
 * @returns 流结束后的 StreamResponse（含所有事件和 sessionId）
 * @throws {IpcConnectionError} 无法解析 Socket 路径时抛出
 */
declare function callDaemonStream(op: string, args: Record<string, unknown> | undefined, callbacks: StreamCallbacks, socketPath?: string): Promise<StreamResponse>;

export { type DaemonError, type DaemonRequest, type DaemonResponse, IpcConnectionError, IpcProtocolError, IpcTimeoutError, StreamCallbacks, StreamEvent, type StreamResponse, callDaemon, callDaemonStream, createConnection, resetClient, resolveSocketPath };
