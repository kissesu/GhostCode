/**
 * @file Daemon 生命周期管理
 * @description 管理 GhostCode Rust Daemon 进程的启动、停止、健康检查和心跳监控。
 *              负责读取 ~/.ghostcode/daemon/ghostcoded.addr.json 获取 Daemon 连接信息，
 *              在 Daemon 未运行时自动 spawn 新进程，并通过心跳维持连接存活。
 *
 *              核心流程：
 *              1. 读取 addr.json -> 检测 Daemon 是否在运行
 *              2. 不存在或 ping 失败 -> spawn ghostcoded 二进制
 *              3. 等待 addr.json 出现（最多 5s）-> ping 确认
 *              4. 心跳每 10s 一次，连续失败 3 次触发重启
 *
 *              参考: crates/ghostcode-types/src/addr.rs - AddrDescriptor 数据结构
 *              参考: crates/ghostcode-types/src/ipc.rs - IPC 协议格式
 *              参考: crates/ghostcode-daemon/src/paths.rs - addr.json 文件路径
 *              参考: oh-my-claudecode/src/features/rate-limit-wait/daemon.ts - spawn 模式
 * @author Atlas.oi
 * @date 2026-03-01
 */
/**
 * 端点描述符
 *
 * Daemon 启动后写入 ghostcoded.addr.json 的连接信息
 * 参考: crates/ghostcode-types/src/addr.rs:25-53
 */
interface AddrDescriptor {
    /** 协议版本号，固定为 1 */
    v: number;
    /** 传输协议，固定为 "unix" */
    transport: string;
    /** Unix socket 文件路径 */
    path: string;
    /** Daemon 进程 ID */
    pid: number;
    /** GhostCode 版本号 */
    version: string;
    /** 启动时间 ISO 8601 UTC */
    ts: string;
}
/**
 * 确保 Daemon 在运行，返回连接地址描述符
 *
 * 并发安全：多个调用者同时调用时，只会 spawn 一次 Daemon。
 * 成功缓存：Daemon 启动成功后缓存 addr，后续调用直接复用（检查进程存活）。
 *
 * @returns Daemon 连接地址描述符
 * @throws 如果 Daemon 启动失败或超时
 */
declare function ensureDaemon(): Promise<AddrDescriptor>;
/**
 * 停止 GhostCode Daemon
 *
 * 通过 IPC 发送 shutdown 操作。如果 Daemon 未运行，静默返回（幂等）。
 */
declare function stopDaemon(): Promise<void>;
/**
 * 启动心跳监控
 *
 * 每 10s 向 Daemon 发送 ping，连续失败 3 次触发重启。
 *
 * @param addr - 当前 Daemon 连接地址
 * @returns 停止心跳的函数
 */
declare function startHeartbeat(addr: AddrDescriptor): () => void;
/**
 * 获取 Daemon 二进制文件路径
 *
 * @returns Daemon 二进制文件的绝对路径
 */
declare function getDaemonBinaryPath(): string;

export { type AddrDescriptor, ensureDaemon, getDaemonBinaryPath, startHeartbeat, stopDaemon };
