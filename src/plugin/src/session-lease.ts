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

import * as fs from "node:fs";
import * as path from "node:path";
import { randomUUID } from "node:crypto";

// ============================================
// 跨进程互斥锁（基于 mkdir 原子性）
// ============================================

// 锁获取最大重试次数
const LOCK_MAX_RETRIES = 100;

// 每次重试间的自旋等待毫秒数
const LOCK_RETRY_SPIN_MS = 10;

// 锁文件超时阈值（毫秒），超过此时间的锁视为宿主进程崩溃遗留
const LOCK_STALE_THRESHOLD_MS = 5000;

/**
 * 基于 mkdir 原子性的跨进程互斥锁
 *
 * 为什么用 mkdir 而不是 flock：
 * - Node.js 没有原生 flock API，引入第三方 lockfile 库会增加运行时依赖
 * - mkdir 在 POSIX 文件系统上是原子操作，创建已存在目录会返回 EEXIST
 * - 锁内临界区非常短（读 JSON + 修改 + 写 JSON），自旋等待开销可忽略
 *
 * 陈旧锁检测：
 * - 如果锁目录的 mtime 超过 LOCK_STALE_THRESHOLD_MS，判定为宿主进程崩溃遗留
 * - 自动清理陈旧锁并重试，防止死锁
 *
 * @param lockPath - 用作互斥锁的目录路径
 * @param fn - 临界区内执行的函数
 * @returns fn 的返回值
 * @throws {Error} 超过最大重试次数后仍无法获取锁
 */
function withFileLock<T>(lockPath: string, fn: () => T): T {
  // ============================================
  // 获取锁：尝试 mkdir，成功即获得互斥权
  // ============================================
  let acquired = false;
  for (let i = 0; i < LOCK_MAX_RETRIES; i++) {
    try {
      fs.mkdirSync(lockPath);
      acquired = true;
      break;
    } catch (err: unknown) {
      const code = (err as NodeJS.ErrnoException).code;
      if (code === "EEXIST") {
        // 锁已被持有，检查是否为陈旧锁
        try {
          const stat = fs.statSync(lockPath);
          if (Date.now() - stat.mtimeMs > LOCK_STALE_THRESHOLD_MS) {
            // 陈旧锁：宿主进程可能已崩溃，强制清理
            try {
              fs.rmdirSync(lockPath);
            } catch {
              // 清理失败（可能被其他进程抢先），继续重试
            }
            continue;
          }
        } catch {
          // stat 失败（锁可能刚被释放），继续重试
        }

        // 自旋等待：临界区非常短，通常 1-2 次重试即可获取
        const end = Date.now() + LOCK_RETRY_SPIN_MS;
        while (Date.now() < end) {
          /* 忙等 */
        }
        continue;
      }
      // 非 EEXIST 错误（权限、磁盘满等），直接抛出
      throw err;
    }
  }

  if (!acquired) {
    throw new Error(
      `获取文件锁超时 (${LOCK_MAX_RETRIES} 次重试后失败): ${lockPath}`,
    );
  }

  // ============================================
  // 执行临界区 + 释放锁（finally 确保释放）
  // ============================================
  try {
    return fn();
  } finally {
    try {
      fs.rmdirSync(lockPath);
    } catch {
      // 释放失败不阻断流程，陈旧锁检测会在下次操作中自动清理
    }
  }
}

// ============================================
// 公开类型定义
// ============================================

/**
 * Lease 获取结果
 */
export interface LeaseInfo {
  /** 唯一的 Lease 标识符（UUID v4 格式） */
  leaseId: string;
  /** 当前总引用计数（包含本次 acquire） */
  refcount: number;
}

/**
 * Lease 释放结果
 */
export interface ReleaseResult {
  /** 释放后的剩余引用计数 */
  refcount: number;
  /** 是否为最后一个 lease（true 表示可以关闭 Daemon） */
  isLast: boolean;
}

// ============================================
// 内部类型：sessions.json 文件结构
// ============================================

/**
 * 单个 session lease 条目
 */
interface SessionEntry {
  leaseId: string;
  pid: number;
  acquiredAt: string;
}

/**
 * sessions.json 文件的根结构
 */
interface SessionsFile {
  sessions: SessionEntry[];
}

// ============================================
// SessionLeaseManager 实现
// ============================================

/**
 * Session Lease 引用计数管理器
 *
 * 使用文件存储实现跨进程安全的引用计数：
 * 1. 每次 acquire/release 操作都包裹在 withFileLock 跨进程互斥锁中
 * 2. 锁内执行 read-modify-write：读文件 → 修改内存 → atomic write 回文件
 * 3. atomic write = 写临时文件 + rename，确保其他进程看到的始终是完整 JSON
 * 4. 如果文件不存在，视为 0 个 session
 */
export class SessionLeaseManager {
  /** sessions.json 文件的绝对路径 */
  private readonly filePath: string;

  /** 互斥锁目录路径（与 sessions.json 同目录下的 .sessions.lock/） */
  private readonly lockPath: string;

  /**
   * 创建 SessionLeaseManager 实例
   *
   * @param sessionsFilePath - sessions.json 文件路径（便于测试时使用临时路径）
   */
  constructor(sessionsFilePath: string) {
    this.filePath = sessionsFilePath;
    // 锁文件放在与 sessions.json 同目录下，使用目录名作为互斥锁
    this.lockPath = path.join(path.dirname(sessionsFilePath), ".sessions.lock");
  }

  // ============================================
  // 公开方法
  // ============================================

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
  acquireLease(): LeaseInfo {
    return withFileLock(this.lockPath, () => {
      const data = this.readFile();

      // 生成新的 UUID lease ID
      const leaseId = randomUUID();

      // 追加新 session 条目
      data.sessions.push({
        leaseId,
        pid: process.pid,
        acquiredAt: new Date().toISOString(),
      });

      // Atomic write 确保文件始终完整（锁保证串行化）
      this.writeFile(data);

      return {
        leaseId,
        refcount: data.sessions.length,
      };
    });
  }

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
  releaseLease(leaseId: string): ReleaseResult {
    return withFileLock(this.lockPath, () => {
      const data = this.readFile();

      // 移除对应 leaseId 的条目（不存在则不影响结果）
      const originalLength = data.sessions.length;
      data.sessions = data.sessions.filter((s) => s.leaseId !== leaseId);
      const wasRemoved = data.sessions.length !== originalLength;

      // 只有真正移除了条目才需要写文件（不存在的 leaseId 不触发写入，但也不报错）
      if (wasRemoved) {
        this.writeFile(data);
      }

      const refcount = data.sessions.length;
      return {
        refcount,
        // 只有剩余 0 个 session 且确实移除了条目时才算"最后一个"
        isLast: refcount === 0 && wasRemoved,
      };
    });
  }

  /**
   * 获取当前引用计数
   *
   * 直接从文件读取，反映最新的跨进程状态。
   *
   * @returns 当前 session 数量（引用计数）
   */
  getRefcount(): number {
    return withFileLock(this.lockPath, () => {
      const data = this.readFile();
      return data.sessions.length;
    });
  }

  // ============================================
  // 私有工具方法
  // ============================================

  /**
   * 读取 sessions.json 文件
   *
   * 文件不存在或解析失败时返回空的 sessions 列表，而不是抛出异常。
   *
   * @returns SessionsFile - 解析后的文件内容
   */
  private readFile(): SessionsFile {
    try {
      const raw = fs.readFileSync(this.filePath, "utf-8");
      const parsed = JSON.parse(raw) as SessionsFile;
      // 确保 sessions 字段存在且为数组
      if (!Array.isArray(parsed.sessions)) {
        return { sessions: [] };
      }
      return parsed;
    } catch {
      // 文件不存在或 JSON 解析失败，返回空状态
      return { sessions: [] };
    }
  }

  /**
   * Atomic write sessions.json 文件
   *
   * 使用"写临时文件 + rename"策略确保跨进程安全：
   * - 直接写目标文件可能导致其他进程读到中间状态（半写入的 JSON）
   * - rename 是原子操作，确保其他进程看到的始终是完整的 JSON
   *
   * @param data - 要写入的文件内容
   */
  private writeFile(data: SessionsFile): void {
    const dir = path.dirname(this.filePath);
    const tmpPath = path.join(dir, `.sessions-tmp-${process.pid}-${Date.now()}.json`);

    // 确保目录存在
    fs.mkdirSync(dir, { recursive: true });

    // 先写临时文件
    fs.writeFileSync(tmpPath, JSON.stringify(data, null, 2), "utf-8");

    // Atomic rename：确保目标文件始终是完整的 JSON
    fs.renameSync(tmpPath, this.filePath);
  }
}
