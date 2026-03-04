/**
 * @file Session Lease 引用计数单元测试
 * @description SessionLeaseManager 的 vitest + fast-check PBT 测试套件。
 *              使用临时目录避免影响系统文件，覆盖 acquire/release 所有边界场景。
 * @author Atlas.oi
 * @date 2026-03-04
 */
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as os from "node:os";
import * as path from "node:path";
import * as fs from "node:fs";
import * as fc from "fast-check";
import { SessionLeaseManager } from "../session-lease.js";

// ============================================
// 测试工具：临时目录管理
// ============================================

/**
 * 创建一个临时测试目录，返回 sessions.json 路径和清理函数
 */
function makeTempSessionsFile(): { sessionsPath: string; cleanup: () => void } {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "ghostcode-lease-test-"));
  const sessionsPath = path.join(tmpDir, "sessions.json");
  return {
    sessionsPath,
    cleanup: () => {
      try {
        fs.rmSync(tmpDir, { recursive: true, force: true });
      } catch {
        // 忽略清理错误
      }
    },
  };
}

// ============================================
// acquireLease 测试
// ============================================

describe("SessionLeaseManager.acquireLease()", () => {
  let sessionsPath: string;
  let cleanup: () => void;
  let manager: SessionLeaseManager;

  beforeEach(() => {
    const tmp = makeTempSessionsFile();
    sessionsPath = tmp.sessionsPath;
    cleanup = tmp.cleanup;
    manager = new SessionLeaseManager(sessionsPath);
  });

  afterEach(() => cleanup());

  it("应该返回 UUID 格式的 leaseId", () => {
    const result = manager.acquireLease();
    // UUID v4 格式：xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx
    expect(result.leaseId).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i
    );
  });

  it("首次 acquire 后 refcount 应为 1", () => {
    const result = manager.acquireLease();
    expect(result.refcount).toBe(1);
  });

  it("连续 acquire 3 次，refcount 应为 3", () => {
    manager.acquireLease();
    manager.acquireLease();
    const result = manager.acquireLease();
    expect(result.refcount).toBe(3);
  });

  it("每次 acquire 返回的 leaseId 应该唯一", () => {
    const ids = [
      manager.acquireLease().leaseId,
      manager.acquireLease().leaseId,
      manager.acquireLease().leaseId,
    ];
    const uniqueIds = new Set(ids);
    expect(uniqueIds.size).toBe(3);
  });
});

// ============================================
// releaseLease 测试
// ============================================

describe("SessionLeaseManager.releaseLease()", () => {
  let sessionsPath: string;
  let cleanup: () => void;
  let manager: SessionLeaseManager;

  beforeEach(() => {
    const tmp = makeTempSessionsFile();
    sessionsPath = tmp.sessionsPath;
    cleanup = tmp.cleanup;
    manager = new SessionLeaseManager(sessionsPath);
  });

  afterEach(() => cleanup());

  it("acquire 3 次后 release 2 次，refcount 应为 1，isLast 应为 false", () => {
    const lease1 = manager.acquireLease();
    const lease2 = manager.acquireLease();
    manager.acquireLease();

    manager.releaseLease(lease1.leaseId);
    const result = manager.releaseLease(lease2.leaseId);

    expect(result.refcount).toBe(1);
    expect(result.isLast).toBe(false);
  });

  it("最后一次 release，isLast 应为 true，refcount 应为 0", () => {
    const lease = manager.acquireLease();
    const result = manager.releaseLease(lease.leaseId);

    expect(result.refcount).toBe(0);
    expect(result.isLast).toBe(true);
  });

  it("release 不存在的 leaseId 应该安全处理（不报错，返回当前 refcount）", () => {
    manager.acquireLease();
    // 不应该抛出异常
    const result = manager.releaseLease("non-existent-lease-id");
    // refcount 保持不变（仍为 1，因为不存在的 lease 不影响计数）
    expect(result.refcount).toBe(1);
    expect(result.isLast).toBe(false);
  });

  it("空状态时 release 不存在的 leaseId 应该安全处理", () => {
    const result = manager.releaseLease("ghost-lease-id");
    expect(result.refcount).toBe(0);
    expect(result.isLast).toBe(false);
  });
});

// ============================================
// getRefcount 测试
// ============================================

describe("SessionLeaseManager.getRefcount()", () => {
  let sessionsPath: string;
  let cleanup: () => void;
  let manager: SessionLeaseManager;

  beforeEach(() => {
    const tmp = makeTempSessionsFile();
    sessionsPath = tmp.sessionsPath;
    cleanup = tmp.cleanup;
    manager = new SessionLeaseManager(sessionsPath);
  });

  afterEach(() => cleanup());

  it("初始状态 refcount 应为 0", () => {
    expect(manager.getRefcount()).toBe(0);
  });

  it("acquire 后 getRefcount 应该与 acquire 返回值一致", () => {
    const r1 = manager.acquireLease();
    expect(manager.getRefcount()).toBe(r1.refcount);

    const r2 = manager.acquireLease();
    expect(manager.getRefcount()).toBe(r2.refcount);
  });
});

// ============================================
// 跨实例持久化测试（验证文件存储）
// ============================================

describe("SessionLeaseManager 文件持久化", () => {
  let sessionsPath: string;
  let cleanup: () => void;

  beforeEach(() => {
    const tmp = makeTempSessionsFile();
    sessionsPath = tmp.sessionsPath;
    cleanup = tmp.cleanup;
  });

  afterEach(() => cleanup());

  it("新实例读取同一文件应该看到之前 acquire 的 lease", () => {
    // 第一个实例 acquire
    const manager1 = new SessionLeaseManager(sessionsPath);
    const lease = manager1.acquireLease();

    // 第二个实例（模拟不同进程）读取同一文件
    const manager2 = new SessionLeaseManager(sessionsPath);
    expect(manager2.getRefcount()).toBe(1);

    // 第二个实例 release
    const result = manager2.releaseLease(lease.leaseId);
    expect(result.isLast).toBe(true);
  });
});

// ============================================
// PBT 属性测试（fast-check）
// ============================================

describe("SessionLeaseManager PBT: refcount 始终 >= 0", () => {
  it("任意 acquire/release 序列，引用计数始终 >= 0", () => {
    fc.assert(
      fc.property(
        // 生成 1-20 次 acquire 操作的序列
        fc.integer({ min: 1, max: 20 }),
        // 生成 release 比例（0.0-1.0），用于随机决定每次是否 release
        fc.array(fc.boolean(), { minLength: 0, maxLength: 40 }),
        (acquireCount, releasePattern) => {
          const tmp = makeTempSessionsFile();
          try {
            const manager = new SessionLeaseManager(tmp.sessionsPath);
            const leaseIds: string[] = [];

            // 执行 acquireCount 次 acquire
            for (let i = 0; i < acquireCount; i++) {
              const result = manager.acquireLease();
              leaseIds.push(result.leaseId);
              // 不变量：acquire 后 refcount 必须 >= 1
              expect(result.refcount).toBeGreaterThanOrEqual(1);
            }

            // 根据 releasePattern 随机 release
            for (let i = 0; i < releasePattern.length && leaseIds.length > 0; i++) {
              if (releasePattern[i]) {
                // 随机选一个 leaseId release
                const idx = i % leaseIds.length;
                // splice 返回数组，此处 leaseIds 非空（外层已做 leaseIds.length > 0 检查）
                const leaseId = leaseIds.splice(idx, 1)[0] as string;
                const result = manager.releaseLease(leaseId);
                // 核心不变量：refcount 始终 >= 0
                expect(result.refcount).toBeGreaterThanOrEqual(0);
              }
            }

            // 最终 refcount 也必须 >= 0
            expect(manager.getRefcount()).toBeGreaterThanOrEqual(0);
          } finally {
            tmp.cleanup();
          }
        }
      ),
      { numRuns: 100 }
    );
  });
});
