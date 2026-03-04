/**
 * @file install-release.test.ts
 * @description 验证从 GitHub Release 下载安装的核心逻辑
 *   测试涵盖：版本检测、URL 生成、checksum 校验、bundle 解压、网络重试
 * @author Atlas.oi
 * @date 2026-03-04
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { existsSync, mkdirSync, writeFileSync, rmSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

// ============================================
// vi.mock 必须在顶层静态声明（vitest hoisting 机制）
// 不能放在 describe 或 beforeEach 内
// ============================================
vi.mock("../utils/downloader.js", () => ({
  downloadWithRetry: vi.fn(),
}));

vi.mock("../utils/checksum.js", () => ({
  verifyChecksum: vi.fn(),
  parseSha256Sums: vi.fn(),
  computeSha256: vi.fn(),
}));

// 在 vi.mock 声明后导入被 mock 的模块
import { downloadWithRetry } from "../utils/downloader.js";
import { verifyChecksum, parseSha256Sums } from "../utils/checksum.js";
import { buildReleaseAssetUrl, installFromRelease } from "../install.js";

// ============================================
// 测试辅助工具
// ============================================

/** 创建临时测试目录 */
function makeTempDir(): string {
  const dir = join(tmpdir(), `ghostcode-test-${Date.now()}-${Math.random().toString(36).slice(2)}`);
  mkdirSync(dir, { recursive: true });
  return dir;
}

/** 创建模拟已安装的标记文件和二进制文件 */
function setupInstalledVersion(targetDir: string, version: string): void {
  // 写入 .installed 标记文件
  const markerPath = join(targetDir, ".installed");
  writeFileSync(markerPath, JSON.stringify({
    version,
    installedAt: new Date().toISOString(),
    platform: "darwin-arm64",
  }), "utf-8");
  // 写入模拟二进制文件（必须存在以通过版本检测）
  writeFileSync(join(targetDir, "ghostcoded"), "fake-binary", "utf-8");
  writeFileSync(join(targetDir, "ghostcode-mcp"), "fake-mcp-binary", "utf-8");
}

// ============================================
// 测试套件
// ============================================

describe("installFromRelease - 版本检测跳过逻辑", () => {
  let tempDir: string;

  beforeEach(() => {
    tempDir = makeTempDir();
    vi.clearAllMocks();
  });

  afterEach(() => {
    if (existsSync(tempDir)) {
      rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it("已安装且版本匹配时不发起网络下载", async () => {
    // 准备：模拟已安装版本 0.2.0
    setupInstalledVersion(tempDir, "0.2.0");

    // 执行：请求安装版本 0.2.0（版本匹配）
    await installFromRelease("0.2.0", "darwin", "arm64", tempDir);

    // 断言：不应发起任何 HTTP 请求
    expect(downloadWithRetry).not.toHaveBeenCalled();
  });
});

describe("buildReleaseAssetUrl - URL 生成", () => {
  it("根据 darwin-arm64 生成正确 Release URL", () => {
    const version = "0.2.0";
    const url = buildReleaseAssetUrl(version, "darwin", "arm64");

    // 验证 URL 格式符合 GitHub Release 规范
    expect(url).toBe(
      `https://github.com/user/GhostCode/releases/download/v${version}/ghostcode-darwin-arm64.tar.gz`
    );
  });

  it("根据 darwin-x64 生成正确 Release URL", () => {
    const version = "1.0.0";
    const url = buildReleaseAssetUrl(version, "darwin", "x64");

    expect(url).toBe(
      `https://github.com/user/GhostCode/releases/download/v${version}/ghostcode-darwin-x64.tar.gz`
    );
  });

  it("根据 linux-x64 生成正确 Release URL", () => {
    const version = "0.5.0";
    const url = buildReleaseAssetUrl(version, "linux", "x64");

    expect(url).toBe(
      `https://github.com/user/GhostCode/releases/download/v${version}/ghostcode-linux-x64.tar.gz`
    );
  });
});

describe("installFromRelease - checksum 校验", () => {
  let tempDir: string;

  beforeEach(() => {
    tempDir = makeTempDir();
    vi.clearAllMocks();
  });

  afterEach(() => {
    if (existsSync(tempDir)) {
      rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it("checksum 不匹配时拒绝安装并保留旧版本", async () => {
    // 准备：已有旧版本
    setupInstalledVersion(tempDir, "0.1.0");
    const oldBinaryContent = "old-binary-content";
    writeFileSync(join(tempDir, "ghostcoded"), oldBinaryContent, "utf-8");

    // mock SHA256SUMS 内容解析返回期望的 hash
    vi.mocked(parseSha256Sums).mockReturnValue("expected-hash-abc123");

    // 模拟下载成功（bundle 和 SHA256SUMS 都下载成功）
    vi.mocked(downloadWithRetry).mockResolvedValue({
      filePath: "/tmp/fake-bundle.tar.gz",
      bytesDownloaded: 1024,
    });

    // 模拟 checksum 不匹配（返回 false）
    vi.mocked(verifyChecksum).mockResolvedValue(false);

    // 执行：请求安装新版本 0.2.0
    // 断言：应抛出错误（禁止静默降级）
    await expect(
      installFromRelease("0.2.0", "darwin", "arm64", tempDir)
    ).rejects.toThrow();

    // 验证旧版本文件内容未被覆盖（checksum 不匹配时应保留原文件）
    const currentContent = readFileSync(join(tempDir, "ghostcoded"), "utf-8");
    expect(currentContent).toBe(oldBinaryContent);
  });
});

describe("installFromRelease - bundle 解压验证", () => {
  let tempDir: string;

  beforeEach(() => {
    tempDir = makeTempDir();
    vi.clearAllMocks();
  });

  afterEach(() => {
    if (existsSync(tempDir)) {
      rmSync(tempDir, { recursive: true, force: true });
    }
  });

  it("checksum 匹配时 installFromRelease 调用了 verifyChecksum", async () => {
    // 此测试验证：当 checksum 匹配时，流程会进入解压阶段
    // 由于真实解压需要有效的 tar.gz 文件，此测试验证 checksum 校验路径

    // mock SHA256SUMS 内容解析（在 parseSha256Sums 被调用前返回期望 hash）
    vi.mocked(parseSha256Sums).mockReturnValue("valid-hash-xyz");

    // 模拟下载成功，并在 destPath 创建真实文件（install.ts 会 readFileSync 这个文件）
    vi.mocked(downloadWithRetry).mockImplementation(async (opts) => {
      // 在目标路径创建一个空文件（install.ts 需要能 readFileSync）
      writeFileSync(opts.destPath, "fake-content", "utf-8");
      return { filePath: opts.destPath, bytesDownloaded: 100 };
    });

    // 模拟 checksum 匹配
    vi.mocked(verifyChecksum).mockResolvedValue(true);

    // 执行安装（解压阶段会失败，因为是空文件无法解压）
    // 我们只验证 verifyChecksum 被调用了（checksum 校验逻辑正常执行）
    try {
      await installFromRelease("0.2.0", "darwin", "arm64", tempDir);
    } catch {
      // 解压阶段预期会失败（因为 fake-content 不是有效的 tar.gz）
      // 这是正常的，我们只关心 verifyChecksum 被调用
    }

    // 验证 verifyChecksum 被调用了（流程正确）
    expect(verifyChecksum).toHaveBeenCalled();
  });

  it("bundle 解压后同时存在 ghostcoded 与 ghostcode-mcp（集成验证）", async () => {
    // 此测试通过创建真实的 tar.gz bundle 来验证解压逻辑
    const { createGzip } = await import("node:zlib");
    const { Readable, Writable } = await import("node:stream");

    // 辅助函数：构建 tar entry（header + data）
    function createTarEntry(name: string, content: Buffer): Buffer {
      // tar header: 512 bytes
      const header = Buffer.alloc(512, 0);
      // 文件名（offset 0, 100 bytes）
      header.write(name, 0, "utf-8");
      // 文件权限（offset 100, 8 bytes）
      header.write("0000755\0", 100, "utf-8");
      // 文件大小（offset 124, 12 bytes）八进制字符串
      const sizeOctal = content.length.toString(8).padStart(11, "0") + "\0";
      header.write(sizeOctal, 124, "utf-8");
      // 修改时间（offset 136, 12 bytes）
      header.write(Math.floor(Date.now() / 1000).toString(8).padStart(11, "0") + "\0", 136, "utf-8");
      // 类型标志（offset 156）: '0' = 普通文件
      header[156] = 0x30; // ASCII '0'
      // 计算 checksum（先用空格填充 checksum 字段）
      for (let i = 148; i < 156; i++) header[i] = 0x20; // 空格
      let checksum = 0;
      for (let i = 0; i < 512; i++) checksum += (header[i] ?? 0);
      // 写入 checksum（offset 148, 8 bytes）
      header.write(checksum.toString(8).padStart(6, "0") + "\0 ", 148, "utf-8");

      // 数据块按 512 字节对齐
      const alignedSize = Math.ceil(content.length / 512) * 512;
      const dataBlock = Buffer.alloc(alignedSize, 0);
      content.copy(dataBlock, 0);

      return Buffer.concat([header, dataBlock]);
    }

    // 构建包含 ghostcoded 和 ghostcode-mcp 的 tar buffer
    const daemonContent = Buffer.from("fake-daemon-binary");
    const mcpContent = Buffer.from("fake-mcp-binary");
    const endBlock = Buffer.alloc(1024, 0); // 两个全零 512 字节 block 作为结尾

    const tarBuffer = Buffer.concat([
      createTarEntry("ghostcoded", daemonContent),
      createTarEntry("ghostcode-mcp", mcpContent),
      endBlock,
    ]);

    // 将 tar buffer 压缩为 gzip，写入临时文件
    const bundlePath = join(tempDir, "test-bundle.tar.gz");
    await new Promise<void>((resolve, reject) => {
      const gzip = createGzip();
      const chunks: Buffer[] = [];
      const inputStream = Readable.from(tarBuffer);
      const collectStream = new Writable({
        write(chunk: Buffer, _enc: BufferEncoding, cb: () => void) {
          chunks.push(chunk);
          cb();
        },
        final(cb: () => void) {
          writeFileSync(bundlePath, Buffer.concat(chunks));
          cb();
        },
      });
      inputStream.pipe(gzip).pipe(collectStream);
      collectStream.on("finish", resolve);
      collectStream.on("error", reject);
      gzip.on("error", reject);
    });

    // mock 下载函数：将 bundle 和 SHA256SUMS 写到实际的 destPath
    vi.mocked(downloadWithRetry).mockImplementation(async (opts) => {
      if (opts.url.endsWith(".tar.gz")) {
        // 将我们创建的 bundle 复制到 destPath（install.ts 将从此路径读取）
        const { copyFileSync } = await import("node:fs");
        copyFileSync(bundlePath, opts.destPath);
        return { filePath: opts.destPath, bytesDownloaded: 2048 };
      }
      // SHA256SUMS 文件：写入 destPath（install.ts 将 readFileSync 此路径）
      writeFileSync(opts.destPath, "fake-hash  ghostcode-darwin-arm64.tar.gz\n", "utf-8");
      return { filePath: opts.destPath, bytesDownloaded: 100 };
    });

    // mock parseSha256Sums 返回期望 hash
    vi.mocked(parseSha256Sums).mockReturnValue("fake-hash");

    // mock checksum 验证通过
    vi.mocked(verifyChecksum).mockResolvedValue(true);

    // 执行安装到独立的安装目标目录
    const installDir = join(tempDir, "install-target");
    mkdirSync(installDir, { recursive: true });

    await installFromRelease("0.2.0", "darwin", "arm64", installDir);

    // 验证解压后同时存在 ghostcoded 和 ghostcode-mcp
    expect(existsSync(join(installDir, "ghostcoded"))).toBe(true);
    expect(existsSync(join(installDir, "ghostcode-mcp"))).toBe(true);
  });
});

describe("downloadWithRetry - 指数退避重试", () => {
  it("网络失败时指数退避重试 3 次", async () => {
    // 追踪请求次数
    let callCount = 0;

    // 替换全局 fetch，前两次失败，第三次成功
    const originalFetch = globalThis.fetch;
    globalThis.fetch = vi.fn().mockImplementation(async () => {
      callCount++;
      if (callCount < 3) {
        throw new Error(`网络错误 (第 ${callCount} 次)`);
      }
      // 第三次成功：返回模拟 Response
      const mockReader = {
        read: vi.fn()
          .mockResolvedValueOnce({ done: false, value: new Uint8Array([1, 2, 3]) })
          .mockResolvedValueOnce({ done: true, value: undefined }),
        releaseLock: vi.fn(),
        cancel: vi.fn(),
        closed: Promise.resolve(undefined),
      };

      return {
        ok: true,
        status: 200,
        headers: {
          get: (key: string) => key === "content-length" ? "3" : null,
        },
        body: {
          getReader: () => mockReader,
        },
      };
    }) as unknown as typeof fetch;

    const tempDir = makeTempDir();
    const destPath = join(tempDir, "test-download");

    try {
      // 动态导入真实的 downloadWithRetry（绕过顶层 mock）
      // 注意：由于顶层 vi.mock，这里需要使用 vi.importActual
      const { downloadWithRetry: realDownload } = await vi.importActual<typeof import("../utils/downloader.js")>(
        "../utils/downloader.js"
      );

      await realDownload({
        url: "https://example.com/test.tar.gz",
        destPath,
        maxRetries: 3,
        baseDelayMs: 10, // 测试环境使用极短延迟（10ms）
      });

      // 验证总共发起 3 次请求（前两次失败 + 第三次成功）
      expect(callCount).toBe(3);
    } finally {
      globalThis.fetch = originalFetch;
      if (existsSync(tempDir)) {
        rmSync(tempDir, { recursive: true, force: true });
      }
    }
  });
});
