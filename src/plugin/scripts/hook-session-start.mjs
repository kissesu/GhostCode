/**
 * @file scripts/hook-session-start.mjs
 * @description SessionStart Hook 脚本
 *              在 Claude Code 会话启动时执行初始化操作：
 *              1. 确保状态目录存在（首次运行时自动创建）
 *              2. 幂等性：如果状态文件已存在（其他 Hook 已初始化），跳过状态文件写入
 *              3. 自动安装 wrapper + prompts（从 Plugin 包 symlink 到 ~/.ghostcode/）
 *              4. 输出 GhostCode Plugin 启动信息（版本 + skill 数量 + Daemon 状态）
 *
 *              与 PreToolUse 的分工：
 *              - SessionStart: 输出欢迎信息，创建目录，标记会话开始，启动 Dashboard Web
 *              - PreToolUse: 启动 Daemon，获取 Session Lease
 *
 *              幂等保证：
 *              - 每次 SessionStart 都输出初始化消息（用于用户感知）
 *              - 状态文件已存在时跳过写入，避免覆盖 PreToolUse 写入的 Daemon 状态
 * @author Atlas.oi
 * @date 2026-03-05
 */

import { existsSync, mkdirSync, writeFileSync, readFileSync, symlinkSync, readlinkSync, unlinkSync, readdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { homedir } from "node:os";
import { fileURLToPath } from "node:url";

// ============================================
// 常量配置
// ============================================

// GhostCode 主目录，支持环境变量覆盖（主要用于测试隔离）
const GHOSTCODE_HOME = process.env.GHOSTCODE_HOME || join(homedir(), ".ghostcode");

// Plugin 版本号（与 package.json 保持一致）
const PLUGIN_VERSION = "0.1.1";

// Hook 状态文件路径（与 hook-pre-tool-use.mjs 和 hook-stop.mjs 共享同一路径）
const STATE_FILE = join(GHOSTCODE_HOME, "state", "hook-state.json");

// Plugin 根目录（由 run.mjs 注入或从脚本路径推导）
const PLUGIN_ROOT = process.env.CLAUDE_PLUGIN_ROOT || join(dirname(fileURLToPath(import.meta.url)), "..");

// ============================================
// Skill 列表：用于计算 skill 数量
// 与 hooks.json 中原 echo 命令的 skill 列表保持一致
// ============================================
const SKILLS = [
  "/gc:team-research",
  "/gc:team-plan",
  "/gc:team-exec",
  "/gc:team-review",
  "/gc:spec-research",
  "/gc:spec-plan",
  "/gc:spec-impl",
];

// ============================================
// 主逻辑
// ============================================

/**
 * 读取 Hook 状态文件
 *
 * @returns {{ daemonStarted: boolean, socketPath: string|null, leaseId: string|null, webStarted: boolean }}
 */
function readState() {
  try {
    if (existsSync(STATE_FILE)) {
      return JSON.parse(readFileSync(STATE_FILE, "utf-8"));
    }
  } catch {
    // 状态文件解析失败时，返回默认状态
  }
  return { daemonStarted: false, socketPath: null, leaseId: null, webStarted: false };
}

/**
 * 写入 Hook 状态文件
 *
 * @param {object} state - 要持久化的状态
 */
function writeState(state) {
  const dir = dirname(STATE_FILE);
  mkdirSync(dir, { recursive: true });
  writeFileSync(STATE_FILE, JSON.stringify(state, null, 2), "utf-8");
}

/**
 * SessionStart Hook 主函数
 *
 * 业务逻辑说明：
 * 1. 确保 GhostCode 状态目录存在（首次安装时创建）
 * 2. 如果状态文件不存在，创建初始空状态（daemonStarted: false）
 *    - 如果状态文件已存在，说明 PreToolUse 已写入 Daemon 状态，跳过写入（幂等保护）
 * 3. 自动安装 wrapper + prompts（从 Plugin 包 symlink 到 ~/.ghostcode/）
 * 4. 启动 Dashboard Web 服务（ensureWeb 单实例保证）
 * 5. 输出初始化消息：版本号 + skill 数量 + Daemon 状态
 */
async function main() {
  // ============================================
  // 第一步：确保状态目录存在
  // 首次运行时自动创建 ~/.ghostcode/state/ 目录
  // ============================================
  const stateDir = join(GHOSTCODE_HOME, "state");
  mkdirSync(stateDir, { recursive: true });

  // ============================================
  // 第二步：幂等性检查 + 初始状态文件创建
  // 仅在状态文件不存在时写入初始状态
  // 避免覆盖 PreToolUse 已写入的 Daemon 启动状态
  // ============================================
  if (!existsSync(STATE_FILE)) {
    // 状态文件不存在，创建初始空状态
    // daemonStarted: false 表示 Daemon 尚未启动（由 PreToolUse 负责启动）
    const initialState = {
      daemonStarted: false,
      socketPath: null,
      leaseId: null,
    };
    writeFileSync(STATE_FILE, JSON.stringify(initialState, null, 2), "utf-8");
  }

  // ============================================
  // 第三步：自动安装 wrapper + prompts
  // 从 Plugin 包 symlink 到 ~/.ghostcode/，Plugin 更新时自动生效
  // ============================================
  setupWrapperAndPrompts();

  // ============================================
  // 第四步：启动 Dashboard Web 服务（单实例保证）
  // 在 SessionStart 阶段就启动 Dashboard，无需等到首次工具调用
  // ensureWeb() 内部有并发保护和幂等检查：
  // - 已运行 → 直接返回（不打开浏览器）
  // - 未运行 → spawn 新进程 → 等待就绪 → 打开浏览器
  // ============================================
  const state = readState();
  if (!state.webStarted) {
    try {
      const { ensureWeb } = await import(join(PLUGIN_ROOT, "dist", "web.js"));
      await ensureWeb();
      state.webStarted = true;
      writeState(state);
    } catch (err) {
      // Dashboard 启动失败不阻断会话建立
      // 用户仍可通过 /gc-web 命令手动启动，或在 PreToolUse 时重试
      console.error("[GhostCode] Dashboard 自动启动失败:", err.message || err);
    }
  }

  // ============================================
  // 第五步：输出初始化消息
  // 每次 SessionStart 都输出，让用户感知 GhostCode 已加载
  // 格式：[GhostCode] Plugin vX.Y.Z | N skills loaded | Daemon: pending
  // ============================================
  const skillCount = SKILLS.length;
  // Daemon 状态固定显示 pending：实际启动由 PreToolUse 负责，此时尚未启动
  const daemonStatus = "pending";
  console.log(`[GhostCode] Plugin v${PLUGIN_VERSION} | ${skillCount} skills loaded | Daemon: ${daemonStatus}`);
}

// ============================================
// setupWrapperAndPrompts 函数
// 从 Plugin 包自动 symlink wrapper 和 prompts 到 ~/.ghostcode/
//
// 业务逻辑：
// 1. 通过 CLAUDE_PLUGIN_ROOT 或脚本路径推算 Plugin 包根目录
// 2. 将 bin/ghostcode-wrapper 启动器 symlink 到 ~/.ghostcode/bin/
// 3. 将 prompts/*.md 角色提示词 symlink 到 ~/.ghostcode/prompts/
// 4. 幂等设计：已存在且指向正确目标的 symlink 不重复创建
// 5. 失败时仅 console.error，不阻断会话建立
// ============================================

/**
 * 创建或更新 symlink（幂等）
 *
 * 业务逻辑说明：
 * 1. 如果 link 已存在且指向正确目标，跳过
 * 2. 如果 link 已存在但指向旧目标，删除后重建
 * 3. 如果 link 不存在，创建新 symlink
 *
 * @param {string} target - symlink 指向的实际文件路径
 * @param {string} linkPath - symlink 文件路径
 * @returns {boolean} 是否成功
 */
function ensureSymlink(target, linkPath) {
  try {
    if (existsSync(linkPath)) {
      // 检查现有 symlink 是否指向正确目标
      try {
        const currentTarget = readlinkSync(linkPath);
        if (currentTarget === target) {
          // 已存在且指向正确目标，跳过（幂等）
          return true;
        }
      } catch {
        // readlinkSync 失败说明不是 symlink（可能是普通文件），删除后重建
      }
      // 删除旧的 symlink 或文件
      unlinkSync(linkPath);
    }
    // 创建新 symlink
    symlinkSync(target, linkPath);
    return true;
  } catch (err) {
    console.error(`[GhostCode] symlink 创建失败: ${linkPath} -> ${target}:`, err.message);
    return false;
  }
}

/**
 * 自动安装 wrapper 和 prompts
 *
 * 两种环境的查找策略：
 * - 分发环境：Plugin 包中有 bin/ghostcode-wrapper 启动器 → 直接 symlink
 * - 开发环境：Plugin 包中无预编译二进制 → 查找 cargo 编译产物 target/release/ghostcode-wrapper
 *             prompts 从 src/plugin/prompts/ 直接 symlink
 *
 * @returns {void}
 */
function setupWrapperAndPrompts() {
  try {
    // ============================================
    // 推算 Plugin 包根目录
    // 优先使用 CLAUDE_PLUGIN_ROOT 环境变量（Claude Code 注入）
    // 回退方案：通过当前脚本路径向上推导（scripts/ -> 根目录）
    // ============================================
    let pluginRoot = process.env.CLAUDE_PLUGIN_ROOT;
    if (!pluginRoot) {
      // 回退：当前脚本在 scripts/ 目录下，向上一级即为 Plugin 根目录
      const currentFile = fileURLToPath(import.meta.url);
      pluginRoot = join(dirname(currentFile), "..");
    }

    // ============================================
    // 确保目标目录存在
    // ============================================
    const binDir = join(GHOSTCODE_HOME, "bin");
    const promptsDir = join(GHOSTCODE_HOME, "prompts");
    mkdirSync(binDir, { recursive: true });
    mkdirSync(promptsDir, { recursive: true });

    // ============================================
    // 查找 wrapper 二进制
    // 策略 1：分发环境 — Plugin 包中的平台检测启动器
    // 策略 2：开发环境 — cargo build 产物 target/release/ghostcode-wrapper
    // ============================================
    let wrapperSrc = join(pluginRoot, "bin", "ghostcode-wrapper");
    if (!existsSync(wrapperSrc)) {
      // 开发环境：从 src/plugin 向上推算项目根目录（../../），查找 cargo 编译产物
      const projectRoot = join(pluginRoot, "..", "..");
      const cargoBinary = join(projectRoot, "target", "release", "ghostcode-wrapper");
      if (existsSync(cargoBinary)) {
        wrapperSrc = cargoBinary;
      } else {
        // 两种路径都找不到 wrapper 二进制，跳过
        // 开发环境需先执行 cargo build --release -p ghostcode-wrapper
        return;
      }
    }

    // symlink wrapper
    // ~/.ghostcode/bin/ghostcode-wrapper -> wrapper 实际路径
    const wrapperLink = join(binDir, "ghostcode-wrapper");
    ensureSymlink(wrapperSrc, wrapperLink);

    // ============================================
    // 查找并 symlink prompts
    // 策略 1：分发环境 — Plugin 包中的 prompts/ 目录
    // 策略 2：开发环境 — src/plugin/prompts/ 目录（pluginRoot 即为 src/plugin）
    // 两种环境下 prompts 都在 ${pluginRoot}/prompts/，逻辑统一
    // ============================================
    const promptsSrcDir = join(pluginRoot, "prompts");
    if (existsSync(promptsSrcDir)) {
      const promptFiles = readdirSync(promptsSrcDir).filter(f => f.endsWith(".md"));
      for (const promptFile of promptFiles) {
        const promptSrc = join(promptsSrcDir, promptFile);
        const promptLink = join(promptsDir, promptFile);
        ensureSymlink(promptSrc, promptLink);
      }
    }
  } catch (err) {
    // 安装失败不阻断会话建立，仅输出错误信息
    console.error("[GhostCode] wrapper/prompts 自动安装失败:", err.message);
  }
}

// ============================================
// 入口：执行异步主逻辑
// exit 0 策略：SessionStart 失败不应阻断 Claude Code 会话建立
// main() 现在是 async（因为需要动态 import ensureWeb），使用 .catch 处理异常
// ============================================
main().catch((err) => {
  console.error("[GhostCode] hook-session-start 初始化失败:", err);
  // exit 0：初始化失败不阻断 Claude Code 正常使用
  process.exit(0);
});
