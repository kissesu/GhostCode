/**
 * @file scripts/hook-pre-compact.mjs
 * @description PreCompact Hook 脚本
 *              在 Claude Code 执行上下文压缩（compact）前触发：
 *              1. 从 stdin 读取压缩前的上下文信息
 *              2. 保存上下文检查点到状态文件（~/.ghostcode/state/compact-checkpoint.json）
 *              3. 检查点内容：压缩时间戳 + 当前活跃子 Agent 数量 + 摘要信息
 *
 *              用途：
 *              - 在上下文压缩后，可通过检查点了解压缩前的状态
 *              - 供 Skill Learning 模块分析压缩频率和原因
 *
 *              注意：失败时 exit 0，不阻断上下文压缩操作
 * @author Atlas.oi
 * @date 2026-03-05
 */

import { existsSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";
import { readStdin } from "./lib/stdin.mjs";

// ============================================
// 常量配置
// ============================================

// GhostCode 主目录，支持环境变量覆盖（主要用于测试隔离）
const GHOSTCODE_HOME = process.env.GHOSTCODE_HOME || join(homedir(), ".ghostcode");

// 压缩检查点文件路径
const CHECKPOINT_FILE = join(GHOSTCODE_HOME, "state", "compact-checkpoint.json");

// 子 Agent 状态文件（读取活跃 Agent 数量）
const SUBAGENTS_FILE = join(GHOSTCODE_HOME, "state", "subagents.json");

// ============================================
// 工具函数
// ============================================

/**
 * 获取当前活跃的子 Agent 数量
 *
 * @returns {number} 活跃子 Agent 数量
 */
function getActiveSubagentCount() {
  try {
    if (existsSync(SUBAGENTS_FILE)) {
      const state = JSON.parse(readFileSync(SUBAGENTS_FILE, "utf-8"));
      return Object.keys(state.agents || {}).length;
    }
  } catch {
    // 读取失败返回 0
  }
  return 0;
}

/**
 * 写入检查点文件
 *
 * @param {object} checkpoint - 检查点数据对象
 */
function writeCheckpoint(checkpoint) {
  try {
    const stateDir = join(GHOSTCODE_HOME, "state");
    mkdirSync(stateDir, { recursive: true });
    writeFileSync(CHECKPOINT_FILE, JSON.stringify(checkpoint, null, 2), "utf-8");
  } catch {
    // 写入失败静默处理
  }
}

// ============================================
// 主逻辑
// ============================================

/**
 * PreCompact Hook 主函数
 *
 * 业务逻辑说明：
 * 1. 从 stdin 读取压缩前的上下文摘要信息
 * 2. 采集当前系统状态（活跃子 Agent 数量等）
 * 3. 生成检查点并写入状态文件
 */
async function main() {
  // ============================================
  // 第一步：从 stdin 读取上下文摘要
  // Claude Code 通过 stdin 传入 PreCompact 事件的 JSON 数据
  // ============================================
  const raw = await readStdin();
  let event = {};
  try {
    event = JSON.parse(raw);
  } catch {
    // JSON 解析失败，使用空事件继续
  }

  // ============================================
  // 第二步：提取摘要信息
  // 兼容嵌套格式：event.event.summary 或 event.summary
  // ============================================
  const inner = event?.event ?? event;
  const summary = inner?.summary || inner?.context_summary || "";
  const triggerReason = inner?.trigger_reason || inner?.triggerReason || "unknown";

  // ============================================
  // 第三步：生成并保存检查点
  // 记录压缩时的系统状态，供后续分析使用
  // ============================================
  const activeSubagents = getActiveSubagentCount();
  const checkpoint = {
    compactedAt: new Date().toISOString(),
    triggerReason,
    summary: summary.slice(0, 500),  // 只保留前 500 字节，避免状态文件过大
    activeSubagents,
  };
  writeCheckpoint(checkpoint);

  console.log(`[GhostCode] PreCompact: 检查点已保存（活跃子 Agent: ${activeSubagents}）`);
}

// ============================================
// 入口：执行主逻辑
// exit 0 策略：检查点保存失败不阻断上下文压缩
// ============================================
main().catch((err) => {
  console.error("[GhostCode] hook-pre-compact 异常:", err);
  process.exit(0);
});
