/**
 * @file Session Lease 引用计数管理器
 * @description 管理多会话共享单 Daemon 时的引用计数。
 *              每个会话 acquire 一个 lease，只有最后一个会话 release 时才真正关闭 Daemon。
 *              Lease 信息存储在文件中，支持跨进程安全读写（mkdir 互斥锁 + atomic write）。
 *
 * 文件格式（sessions.json）：
 * {
 *   "sessions": [
 *     { "leaseId": "uuid-1", "pid": 12345, "acquiredAt": "ISO-timestamp" },
 *     { "leaseId": "uuid-2", "pid": 12346, "acquiredAt": "ISO-timestamp" }
 *   ]
 * }
 *
 * @author Atlas.oi
 * @date 2026-03-04
 */
/**
 * Lease 获取结果
 */
interface LeaseInfo {
    /** 唯一的 Lease 标识符（UUID v4 格式） */
    leaseId: string;
    /** 当前总引用计数（包含本次 acquire） */
    refcount: number;
}
/**
 * Lease 释放结果
 */
interface ReleaseResult {
    /** 释放后的剩余引用计数 */
    refcount: number;
    /** 是否为最后一个 lease（true 表示可以关闭 Daemon） */
    isLast: boolean;
}
/**
 * Session Lease 引用计数管理器
 *
 * 使用文件存储实现跨进程安全的引用计数：
 * 1. 每次 acquire/release 操作都包裹在 withFileLock 跨进程互斥锁中
 * 2. 锁内执行 read-modify-write：读文件 → 修改内存 → atomic write 回文件
 * 3. atomic write = 写临时文件 + rename，确保其他进程看到的始终是完整 JSON
 * 4. 如果文件不存在，视为 0 个 session
 */
declare class SessionLeaseManager {
    /** sessions.json 文件的绝对路径 */
    private readonly filePath;
    /** 互斥锁目录路径（与 sessions.json 同目录下的 .sessions.lock/） */
    private readonly lockPath;
    /**
     * 创建 SessionLeaseManager 实例
     *
     * @param sessionsFilePath - sessions.json 文件路径（便于测试时使用临时路径）
     */
    constructor(sessionsFilePath: string);
    /**
     * 获取一个 session lease（引用计数 +1）
     *
     * 业务逻辑：
     * 1. 读取当前 sessions.json 文件（不存在则视为空列表）
     * 2. 生成新 UUID 作为 leaseId
     * 3. 将新 session 追加到列表
     * 4. Atomic write 回文件
     * 5. 返回新 leaseId 和最新 refcount
     *
     * @returns LeaseInfo - 包含 leaseId 和当前 refcount
     */
    acquireLease(): LeaseInfo;
    /**
     * 释放一个 session lease（引用计数 -1）
     *
     * 业务逻辑：
     * 1. 读取当前 sessions.json 文件
     * 2. 从列表中移除对应 leaseId 的条目（找不到则安全跳过）
     * 3. Atomic write 回文件
     * 4. 返回剩余 refcount 和 isLast 标志
     *
     * @param leaseId - 要释放的 lease 标识符
     * @returns ReleaseResult - 包含剩余 refcount 和 isLast 标志
     */
    releaseLease(leaseId: string): ReleaseResult;
    /**
     * 获取当前引用计数
     *
     * 直接从文件读取，反映最新的跨进程状态。
     *
     * @returns 当前 session 数量（引用计数）
     */
    getRefcount(): number;
    /**
     * 读取 sessions.json 文件
     *
     * 文件不存在或解析失败时返回空的 sessions 列表，而不是抛出异常。
     *
     * @returns SessionsFile - 解析后的文件内容
     */
    private readFile;
    /**
     * Atomic write sessions.json 文件
     *
     * 使用"写临时文件 + rename"策略确保跨进程安全：
     * - 直接写目标文件可能导致其他进程读到中间状态（半写入的 JSON）
     * - rename 是原子操作，确保其他进程看到的始终是完整的 JSON
     *
     * @param data - 要写入的文件内容
     */
    private writeFile;
}

export { type LeaseInfo, type ReleaseResult, SessionLeaseManager };
