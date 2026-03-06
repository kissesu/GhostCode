/**
 * @file install-wrapper.mjs
 * @description ghostcode-wrapper 安装脚本
 *              构建 Rust 二进制并部署到 ~/.ghostcode/bin/，
 *              同时复制角色提示词到 ~/.ghostcode/prompts/
 *
 *              执行流程：
 *              1. 检测项目根目录（通过查找 Cargo.toml）
 *              2. 执行 cargo build --release -p ghostcode-wrapper
 *              3. 创建目标目录 ~/.ghostcode/bin/ 和 ~/.ghostcode/prompts/
 *              4. 复制二进制到 ~/.ghostcode/bin/ghostcode-wrapper
 *              5. 复制角色提示词 src/plugin/prompts/*.md 到 ~/.ghostcode/prompts/
 *              6. 设置二进制执行权限（chmod +x）
 *              7. 运行 ghostcode-wrapper --help 验证安装成功
 *              8. 探测各 CLI 后端可用性（codex / gemini / claude）
 * @author Atlas.oi
 * @date 2026-03-06
 */

import {
  existsSync,
  mkdirSync,
  copyFileSync,
  chmodSync,
  readdirSync,
} from "node:fs";
import { join, dirname, basename } from "node:path";
import { homedir } from "node:os";
import { execSync, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

// ============================================
// 路径常量
// ============================================

// 当前脚本所在目录：src/plugin/scripts/
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// GhostCode 主目录，支持环境变量覆盖（主要用于测试隔离）
const GHOSTCODE_HOME = process.env.GHOSTCODE_HOME || join(homedir(), ".ghostcode");

// 目标安装目录
const BIN_DIR = join(GHOSTCODE_HOME, "bin");
const PROMPTS_DIR = join(GHOSTCODE_HOME, "prompts");

// ============================================
// 工具函数
// ============================================

/**
 * 向上查找含有 Cargo.toml 的项目根目录
 *
 * 业务逻辑说明：
 * 1. 从脚本所在目录开始，逐层向上查找
 * 2. 每一层检查是否存在 Cargo.toml 文件
 * 3. 找到则返回该目录，否则继续向上，直到文件系统根目录
 *
 * @param {string} startDir - 查找的起始目录
 * @returns {string} 项目根目录路径
 * @throws {Error} 找不到 Cargo.toml 时抛出
 */
function findProjectRoot(startDir) {
  let current = startDir;
  while (true) {
    if (existsSync(join(current, "Cargo.toml"))) {
      return current;
    }
    const parent = dirname(current);
    // 到达文件系统根目录，停止查找
    if (parent === current) {
      throw new Error("未找到 Cargo.toml，请确保从项目目录执行安装脚本");
    }
    current = parent;
  }
}

/**
 * 探测命令行工具是否可用，并获取其版本号
 *
 * 业务逻辑说明：
 * 1. 使用 spawnSync 执行 <cmd> --version
 * 2. 若执行成功，解析版本号字符串（去除多余空白）
 * 3. 若执行失败或命令不存在，返回 null 表示不可用
 *
 * @param {string} cmd - 命令行工具名称（如 "codex"、"claude"）
 * @returns {{ available: boolean; version: string | null }} 可用性与版本信息
 */
function probeCli(cmd) {
  try {
    const result = spawnSync(cmd, ["--version"], {
      encoding: "utf-8",
      timeout: 5000,
    });
    if (result.status === 0) {
      // 提取第一行版本信息，去除多余空白
      const version = (result.stdout || result.stderr || "").trim().split("\n")[0].trim();
      return { available: true, version: version || "未知版本" };
    }
    return { available: false, version: null };
  } catch {
    return { available: false, version: null };
  }
}

// ============================================
// 主安装逻辑
// ============================================

/**
 * 安装脚本主函数
 *
 * 业务逻辑说明：
 * 1. 定位项目根目录
 * 2. 执行 cargo build 构建 ghostcode-wrapper 发布版本
 * 3. 创建安装目录并复制文件
 * 4. 设置权限并验证安装
 * 5. 探测 CLI 后端可用性
 */
function main() {
  // ============================================
  // 第一步：定位项目根目录
  // 通过查找 Cargo.toml 确保从任意位置执行脚本都能正确找到项目
  // ============================================
  let projectRoot;
  try {
    projectRoot = findProjectRoot(__dirname);
  } catch (err) {
    console.error(`[GhostCode] 错误：${err.message}`);
    process.exit(1);
  }

  // ============================================
  // 第二步：执行 cargo build 构建 ghostcode-wrapper
  // 使用 --release 编译优化版本以获得最佳性能
  // ============================================
  console.log("[GhostCode] 开始构建 ghostcode-wrapper...");
  console.log("[GhostCode] cargo build --release -p ghostcode-wrapper");

  try {
    execSync("cargo build --release -p ghostcode-wrapper", {
      cwd: projectRoot,
      stdio: "inherit",
      encoding: "utf-8",
    });
  } catch {
    console.error("[GhostCode] 错误：cargo build 失败，请检查 Rust 环境和编译错误");
    process.exit(1);
  }

  console.log("[GhostCode] 构建成功");

  // ============================================
  // 第三步：创建目标目录
  // 确保 ~/.ghostcode/bin/ 和 ~/.ghostcode/prompts/ 存在
  // ============================================
  mkdirSync(BIN_DIR, { recursive: true });
  mkdirSync(PROMPTS_DIR, { recursive: true });

  // ============================================
  // 第四步：复制二进制到安装目录
  // 源路径：target/release/ghostcode-wrapper
  // 目标路径：~/.ghostcode/bin/ghostcode-wrapper
  // ============================================
  const binaryName = "ghostcode-wrapper";
  const sourceBinary = join(projectRoot, "target", "release", binaryName);
  const targetBinary = join(BIN_DIR, binaryName);

  if (!existsSync(sourceBinary)) {
    console.error(`[GhostCode] 错误：构建产物不存在：${sourceBinary}`);
    process.exit(1);
  }

  copyFileSync(sourceBinary, targetBinary);
  console.log(`[GhostCode] 安装二进制到 ~/.ghostcode/bin/ghostcode-wrapper`);

  // ============================================
  // 第五步：复制角色提示词到 ~/.ghostcode/prompts/
  // 来源：src/plugin/prompts/*.md
  // ============================================
  const promptsSourceDir = join(projectRoot, "src", "plugin", "prompts");

  if (existsSync(promptsSourceDir)) {
    const mdFiles = readdirSync(promptsSourceDir).filter((f) => f.endsWith(".md"));
    for (const file of mdFiles) {
      const src = join(promptsSourceDir, file);
      const dest = join(PROMPTS_DIR, basename(file));
      copyFileSync(src, dest);
    }
    console.log(`[GhostCode] 复制角色提示词到 ~/.ghostcode/prompts/`);
  } else {
    console.warn(`[GhostCode] 警告：提示词目录不存在：${promptsSourceDir}`);
  }

  // ============================================
  // 第六步：设置二进制执行权限
  // chmod +x 确保二进制可直接执行
  // ============================================
  chmodSync(targetBinary, 0o755);

  console.log("[GhostCode] 安装完成");

  // ============================================
  // 第七步：执行 runtime probe 验证安装成功
  // 直接运行已安装的二进制文件进行验证
  // ============================================
  const probe = spawnSync(targetBinary, ["--help"], {
    encoding: "utf-8",
    timeout: 10000,
  });

  if (probe.status !== 0) {
    console.error("[GhostCode] 错误：安装验证失败，ghostcode-wrapper --help 执行异常");
    console.error(probe.stderr || "无错误输出");
    process.exit(1);
  }

  // ============================================
  // 第八步：探测各 CLI 后端可用性
  // 各 CLI 工具并非全部必须可用，不可用时仅输出警告
  // ============================================
  console.log("[GhostCode] CLI 可用性检测:");

  const cliTools = [
    { cmd: "codex", label: "Codex" },
    { cmd: "gemini", label: "Gemini" },
    { cmd: "claude", label: "Claude" },
  ];

  for (const { cmd, label } of cliTools) {
    const { available, version } = probeCli(cmd);
    if (available) {
      console.log(`  - ${label}: 可用 (${version})`);
    } else {
      console.warn(`  - ${label}: 不可用（未安装或不在 PATH 中）`);
    }
  }
}

// ============================================
// 入口：执行主安装逻辑
// ============================================
main();
