/**
 * @file scripts/hook-session-end.mjs
 * @description SessionEnd Hook 脚本
 *              在 Claude Code 会话正常结束时执行清理操作：
 *              1. 触发最终 Skill Learning 汇总（调用 learner 模块的 onSessionEnd）
 *              2. 清理 ~/.ghostcode/state/ 下的状态文件（重置会话状态）
 *
 *              与 Stop Hook 的分工：
 *              - Stop: 在 Claude Code 输出停止时触发，负责释放 Lease + 关闭 Daemon
 *              - SessionEnd: 在整个会话结束时触发，负责最终汇总 + 状态清理
 *
 *              注意：失败时 exit 0，不阻断 Claude Code 会话正常关闭
 * @author Atlas.oi
 * @date 2026-03-05
 */

import { existsSync, readdirSync, unlinkSync, mkdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { homedir } from "node:os";

// ============================================
// 常量配置
// ============================================

// GhostCode 主目录，支持环境变量覆盖（主要用于测试隔离）
const GHOSTCODE_HOME = process.env.GHOSTCODE_HOME || join(homedir(), ".ghostcode");

// 状态目录路径
const STATE_DIR = join(GHOSTCODE_HOME, "state");

// Plugin 根目录（用于动态 import learner 模块）
const PLUGIN_ROOT = process.env.CLAUDE_PLUGIN_ROOT || join(dirname(new URL(import.meta.url).pathname), "..");

// ============================================
// 状态清理工具函数
// ============================================

/**
 * 清理 state 目录下的所有状态文件
 *
 * 业务逻辑说明：
 * 1. 检查 state 目录是否存在
 * 2. 遍历目录下所有 .json 文件并删除
 * 3. 失败时静默处理，不阻断主流程
 */
function clearStateFiles() {
  try {
    if (!existsSync(STATE_DIR)) {
      return;
    }
    const files = readdirSync(STATE_DIR);
    for (const file of files) {
      if (file.endsWith(".json")) {
        try {
          unlinkSync(join(STATE_DIR, file));
        } catch {
          // 单个文件删除失败不影响其他文件
        }
      }
    }
  } catch {
    // state 目录读取失败，静默处理
  }
}

// ============================================
// 主逻辑
// ============================================

/**
 * SessionEnd Hook 主函数
 *
 * 业务逻辑说明：
 * 1. 触发 Skill Learning 最终汇总（onSessionEnd）
 *    - 调用成功：输出汇总日志
 *    - 调用失败：静默处理，继续执行清理
 * 2. 清理 state 目录下的所有状态文件
 */
async function main() {
  // ============================================
  // 第一步：触发 Skill Learning 最终汇总
  // 确保会话结束时所有学习内容被持久化
  // ============================================
  try {
    const { onSessionEnd } = await import(join(PLUGIN_ROOT, "dist", "learner", "index.js"));
    await onSessionEnd();
    console.log("[GhostCode] SessionEnd: Skill Learning 汇总完成");
  } catch {
    // learner 模块可能未构建（开发环境），静默处理
  }

  // ============================================
  // 第二步：清理状态文件
  // 会话已完全结束，重置所有 Hook 状态
  // ============================================
  clearStateFiles();
  console.log("[GhostCode] SessionEnd: 状态文件清理完成");
}

// ============================================
// 入口：执行主逻辑
// exit 0 策略：SessionEnd 失败不应阻断 Claude Code 会话关闭
// ============================================
main().catch((err) => {
  console.error("[GhostCode] hook-session-end 异常:", err);
  clearStateFiles();
  process.exit(0);
});
