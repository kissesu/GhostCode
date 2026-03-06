/**
 * @file scripts/hook-subagent-stop.mjs
 * @description SubagentStop Hook 脚本
 *              在子 Agent 停止时执行：
 *              1. 从 stdin 读取子 Agent 事件信息（agent ID 等）
 *              2. 从状态文件中移除该子 Agent 的记录
 *              3. 向 Daemon 注销 Actor（IPC actor_stop），使 Dashboard 实时感知
 *
 *              注意：失败时 exit 0，不阻断子 Agent 正常停止
 * @author Atlas.oi
 * @date 2026-03-06
 */

import { existsSync, readFileSync, writeFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";
import { readStdin } from "./lib/stdin.mjs";

// ============================================
// 常量配置
// ============================================

// GhostCode 主目录，支持环境变量覆盖（主要用于测试隔离）
const GHOSTCODE_HOME = process.env.GHOSTCODE_HOME || join(homedir(), ".ghostcode");

// 子 Agent 状态文件路径（与 hook-subagent-start.mjs 共享）
const SUBAGENTS_FILE = join(GHOSTCODE_HOME, "state", "subagents.json");

// ============================================
// 状态文件读写工具函数
// ============================================

/**
 * 读取子 Agent 状态文件
 *
 * @returns {{ agents: Record<string, unknown> }} 子 Agent 状态
 */
function readSubagentsState() {
  try {
    if (existsSync(SUBAGENTS_FILE)) {
      return JSON.parse(readFileSync(SUBAGENTS_FILE, "utf-8"));
    }
  } catch {
    // 解析失败返回空状态
  }
  return { agents: {} };
}

/**
 * 写入子 Agent 状态文件
 *
 * @param {{ agents: Record<string, unknown> }} state - 要写入的状态对象
 */
function writeSubagentsState(state) {
  try {
    writeFileSync(SUBAGENTS_FILE, JSON.stringify(state, null, 2), "utf-8");
  } catch {
    // 写入失败静默处理
  }
}

/**
 * 从 ~/.ghostcode/groups/ 目录解析当前活跃的 group_id
 *
 * 简单策略：取第一个 g- 开头的目录名（当前单 group 场景）
 *
 * @returns {string|null} group_id 或 null
 */
function resolveGroupId() {
  try {
    const groupsDir = join(GHOSTCODE_HOME, "groups");
    if (!existsSync(groupsDir)) return null;
    const dirs = readdirSync(groupsDir, { withFileTypes: true })
      .filter((d) => d.isDirectory() && d.name.startsWith("g-"))
      .map((d) => d.name);
    return dirs.length > 0 ? dirs[0] : null;
  } catch {
    return null;
  }
}

// ============================================
// 主逻辑
// ============================================

/**
 * SubagentStop Hook 主函数
 *
 * 业务逻辑说明：
 * 1. 从 stdin 读取 Claude Code 传入的事件 JSON
 * 2. 提取子 Agent ID
 * 3. 从 subagents.json 状态文件中删除该 Agent 记录
 * 4. 向 Daemon 注销 Actor（IPC actor_stop）
 */
async function main() {
  // ============================================
  // 第一步：从 stdin 读取事件数据
  // Claude Code 通过 stdin 传入 SubagentStop 事件的 JSON 数据
  // ============================================
  const raw = await readStdin();
  let event = {};
  try {
    event = JSON.parse(raw);
  } catch {
    // JSON 解析失败，使用空事件继续
  }

  // ============================================
  // 第二步：提取子 Agent ID
  // 兼容嵌套格式：event.event.agentId 或 event.agentId
  // ============================================
  const inner = event?.event ?? event;
  const agentId = inner?.agent_id || inner?.agentId || "";

  // ============================================
  // 第三步：从状态文件移除子 Agent 记录
  // 不影响其他仍在运行的子 Agent
  // ============================================
  const state = readSubagentsState();
  if (agentId && state.agents[agentId]) {
    delete state.agents[agentId];
    writeSubagentsState(state);
    console.log(`[GhostCode] SubagentStop: 已移除子 Agent ${agentId} 的记录`);
  } else {
    // agentId 不存在于状态文件（可能从未记录），静默处理
    console.log(`[GhostCode] SubagentStop: 子 Agent ${agentId || "(未知)"} 已停止`);
  }

  // ============================================
  // 第四步：向 Daemon 注销 Actor（Daemon 在运行时）
  // 通过 IPC 调用 actor_stop，让 Daemon 感知 Agent 已停止
  // Daemon 会将事件写入 Ledger，Dashboard 通过 SSE 实时更新
  //
  // 注意：Hook 失败 exit 0 是 Claude Code Plugin 规范要求，
  // 不是降级策略——Hook 失败不应阻断 Claude Code 本身运行
  // ============================================
  if (agentId) {
    try {
      const { callDaemon } = await import("./lib/daemon-client.mjs");

      const groupId = resolveGroupId();
      if (groupId) {
        const resp = await callDaemon("actor_stop", {
          group_id: groupId,
          actor_id: agentId,
          by: "system",
        });
        if (resp?.ok) {
          console.log(`[GhostCode] SubagentStop: Actor ${agentId} 已向 Daemon 注销`);
        } else {
          console.error(`[GhostCode] SubagentStop: Daemon actor_stop 失败:`, resp?.error?.message || "未知错误");
        }
      }
    } catch (err) {
      // Daemon IPC 失败不阻断子 Agent 停止（Hook exit 0 规范）
      console.error("[GhostCode] SubagentStop: Daemon IPC 异常:", err.message);
    }
  }
}

// ============================================
// 入口：执行主逻辑
// exit 0 策略：清理失败不阻断子 Agent 正常停止
// ============================================
main().catch((err) => {
  console.error("[GhostCode] hook-subagent-stop 异常:", err);
  process.exit(0);
});
