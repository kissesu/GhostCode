import * as fs from "node:fs";
import * as path from "node:path";
import { randomUUID } from "node:crypto";
const LOCK_MAX_RETRIES = 100;
const LOCK_RETRY_SPIN_MS = 10;
const LOCK_STALE_THRESHOLD_MS = 5e3;
function withFileLock(lockPath, fn) {
  let acquired = false;
  for (let i = 0; i < LOCK_MAX_RETRIES; i++) {
    try {
      fs.mkdirSync(lockPath);
      acquired = true;
      break;
    } catch (err) {
      const code = err.code;
      if (code === "EEXIST") {
        try {
          const stat = fs.statSync(lockPath);
          if (Date.now() - stat.mtimeMs > LOCK_STALE_THRESHOLD_MS) {
            try {
              fs.rmdirSync(lockPath);
            } catch {
            }
            continue;
          }
        } catch {
        }
        const end = Date.now() + LOCK_RETRY_SPIN_MS;
        while (Date.now() < end) {
        }
        continue;
      }
      throw err;
    }
  }
  if (!acquired) {
    throw new Error(
      `\u83B7\u53D6\u6587\u4EF6\u9501\u8D85\u65F6 (${LOCK_MAX_RETRIES} \u6B21\u91CD\u8BD5\u540E\u5931\u8D25): ${lockPath}`
    );
  }
  try {
    return fn();
  } finally {
    try {
      fs.rmdirSync(lockPath);
    } catch {
    }
  }
}
class SessionLeaseManager {
  /** sessions.json 文件的绝对路径 */
  filePath;
  /** 互斥锁目录路径（与 sessions.json 同目录下的 .sessions.lock/） */
  lockPath;
  /**
   * 创建 SessionLeaseManager 实例
   *
   * @param sessionsFilePath - sessions.json 文件路径（便于测试时使用临时路径）
   */
  constructor(sessionsFilePath) {
    this.filePath = sessionsFilePath;
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
  acquireLease() {
    return withFileLock(this.lockPath, () => {
      const data = this.readFile();
      const leaseId = randomUUID();
      data.sessions.push({
        leaseId,
        pid: process.pid,
        acquiredAt: (/* @__PURE__ */ new Date()).toISOString()
      });
      this.writeFile(data);
      return {
        leaseId,
        refcount: data.sessions.length
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
  releaseLease(leaseId) {
    return withFileLock(this.lockPath, () => {
      const data = this.readFile();
      const originalLength = data.sessions.length;
      data.sessions = data.sessions.filter((s) => s.leaseId !== leaseId);
      const wasRemoved = data.sessions.length !== originalLength;
      if (wasRemoved) {
        this.writeFile(data);
      }
      const refcount = data.sessions.length;
      return {
        refcount,
        // 只有剩余 0 个 session 且确实移除了条目时才算"最后一个"
        isLast: refcount === 0 && wasRemoved
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
  getRefcount() {
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
  readFile() {
    try {
      const raw = fs.readFileSync(this.filePath, "utf-8");
      const parsed = JSON.parse(raw);
      if (!Array.isArray(parsed.sessions)) {
        return { sessions: [] };
      }
      return parsed;
    } catch {
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
  writeFile(data) {
    const dir = path.dirname(this.filePath);
    const tmpPath = path.join(dir, `.sessions-tmp-${process.pid}-${Date.now()}.json`);
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(tmpPath, JSON.stringify(data, null, 2), "utf-8");
    fs.renameSync(tmpPath, this.filePath);
  }
}
export {
  SessionLeaseManager
};
//# sourceMappingURL=session-lease.js.map