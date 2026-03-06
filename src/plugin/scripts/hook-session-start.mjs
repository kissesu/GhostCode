/**
 * @file scripts/hook-session-start.mjs
 * @description SessionStart Hook 脚本
 *              在 Claude Code 会话启动时执行初始化操作：
 *              1. 输出 GhostCode Plugin 启动信息（版本 + skill 数量 + Daemon 状态）
 *              2. 确保状态目录存在（首次运行时自动创建）
 *              3. 幂等性：如果状态文件已存在（其他 Hook 已初始化），跳过状态文件写入
 *
 *              与 PreToolUse 的分工：
 *              - SessionStart: 输出欢迎信息，创建目录，标记会话开始
 *              - PreToolUse: 启动 Daemon，获取 Session Lease
 *
 *              幂等保证：
 *              - 每次 SessionStart 都输出初始化消息（用于用户感知）
 *              - 状态文件已存在时跳过写入，避免覆盖 PreToolUse 写入的 Daemon 状态
 * @author Atlas.oi
 * @date 2026-03-05
 */

import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { homedir } from "node:os";

// ============================================
// 常量配置
// ============================================

// GhostCode 主目录，支持环境变量覆盖（主要用于测试隔离）
const GHOSTCODE_HOME = process.env.GHOSTCODE_HOME || join(homedir(), ".ghostcode");

// Plugin 版本号（与 package.json 保持一致）
const PLUGIN_VERSION = "0.1.0";

// Hook 状态文件路径（与 hook-pre-tool-use.mjs 和 hook-stop.mjs 共享同一路径）
const STATE_FILE = join(GHOSTCODE_HOME, "state", "hook-state.json");

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
 * SessionStart Hook 主函数
 *
 * 业务逻辑说明：
 * 1. 确保 GhostCode 状态目录存在（首次安装时创建）
 * 2. 如果状态文件不存在，创建初始空状态（daemonStarted: false）
 *    - 如果状态文件已存在，说明 PreToolUse 已写入 Daemon 状态，跳过写入（幂等保护）
 * 3. 输出初始化消息：版本号 + skill 数量 + Daemon 状态
 */
function main() {
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
  // 第三步：输出初始化消息
  // 每次 SessionStart 都输出，让用户感知 GhostCode 已加载
  // 格式：[GhostCode] Plugin vX.Y.Z | N skills loaded | Daemon: pending
  // ============================================
  const skillCount = SKILLS.length;
  // Daemon 状态固定显示 pending：实际启动由 PreToolUse 负责，此时尚未启动
  const daemonStatus = "pending";
  console.log(`[GhostCode] Plugin v${PLUGIN_VERSION} | ${skillCount} skills loaded | Daemon: ${daemonStatus}`);
}

// ============================================
// 入口：执行主逻辑
// exit 0 策略：SessionStart 失败不应阻断 Claude Code 会话建立
// ============================================
try {
  main();
} catch (err) {
  console.error("[GhostCode] hook-session-start 初始化失败:", err);
  // exit 0：初始化失败不阻断 Claude Code 正常使用
  process.exit(0);
}
