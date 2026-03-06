/**
 * @file scripts/hook-subagent-start.mjs
 * @description SubagentStart Hook 脚本
 *              在子 Agent 启动时执行：
 *              1. 从 stdin 读取子 Agent 事件信息（agent ID、session ID 等）
 *              2. 将子 Agent 记录写入状态文件，供其他 Hook 感知当前活跃的子 Agent
 *              3. 向 Daemon 注册 Actor（IPC actor_start），使 Dashboard 实时感知
 *
 *              状态文件结构（~/.ghostcode/state/subagents.json）：
 *              {
 *                "agents": {
 *                  "<agentId>": {
 *                    "startedAt": "<ISO时间>",
 *                    "sessionId": "<会话ID>"
 *                  }
 *                }
 *              }
 *
 *              注意：失败时 exit 0，不阻断子 Agent 启动
 * @author Atlas.oi
 * @date 2026-03-06
 */

import { existsSync, readFileSync, writeFileSync, mkdirSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";
import { readStdin } from "./lib/stdin.mjs";

// ============================================
// 常量配置
// ============================================

// GhostCode 主目录，支持环境变量覆盖（主要用于测试隔离）
const GHOSTCODE_HOME = process.env.GHOSTCODE_HOME || join(homedir(), ".ghostcode");

// 子 Agent 状态文件路径
const SUBAGENTS_FILE = join(GHOSTCODE_HOME, "state", "subagents.json");

// ============================================
// 状态文件读写工具函数
// ============================================

/**
 * 读取子 Agent 状态文件
 *
 * @returns {{ agents: Record<string, { startedAt: string; sessionId: string }> }} 子 Agent 状态
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
    const stateDir = join(GHOSTCODE_HOME, "state");
    mkdirSync(stateDir, { recursive: true });
    writeFileSync(SUBAGENTS_FILE, JSON.stringify(state, null, 2), "utf-8");
  } catch {
    // 写入失败静默处理
  }
}

/**
 * 从 ~/.ghostcode/groups/ 目录解析当前活跃的 group_id
 *
 * 简单策略：取第一个 g- 开头的目录名（当前单 group 场景）
 * 后续多 group 场景可通过 hook-state.json 中的 activeGroupId 扩展
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
 * SubagentStart Hook 主函数
 *
 * 业务逻辑说明：
 * 1. 从 stdin 读取 Claude Code 传入的事件 JSON
 * 2. 提取子 Agent ID 和会话 ID
 * 3. 将子 Agent 记录追加到 subagents.json 状态文件
 * 4. 向 Daemon 注册 Actor（IPC actor_start）
 */
async function main() {
  // ============================================
  // 第一步：从 stdin 读取事件数据
  // Claude Code 通过 stdin 传入 SubagentStart 事件的 JSON 数据
  // ============================================
  const raw = await readStdin();
  let event = {};
  try {
    event = JSON.parse(raw);
  } catch {
    // JSON 解析失败，使用空事件继续（可能无 stdin 输入）
  }

  // ============================================
  // 第二步：提取子 Agent 信息
  // 兼容嵌套格式：event.event.agentId 或 event.agentId
  // ============================================
  const inner = event?.event ?? event;
  const agentId = inner?.agent_id || inner?.agentId || `agent-${Date.now()}`;
  const sessionId = inner?.session_id || inner?.sessionId || "";

  // ============================================
  // 第三步：记录子 Agent 到状态文件
  // 追加到已有的 agents 记录，不覆盖其他活跃 Agent
  // ============================================
  const state = readSubagentsState();
  state.agents[agentId] = {
    startedAt: new Date().toISOString(),
    sessionId,
  };
  writeSubagentsState(state);

  console.log(`[GhostCode] SubagentStart: 已记录子 Agent ${agentId}`);

  // ============================================
  // 第四步：向 Daemon 注册 Actor（Daemon 在运行时）
  // 通过 IPC 调用 actor_start，让 Daemon 感知新 Agent 的存在
  // Daemon 会将事件写入 Ledger，Dashboard 通过 SSE 实时显示
  //
  // 注意：Hook 失败 exit 0 是 Claude Code Plugin 规范要求，
  // 不是降级策略——Hook 失败不应阻断 Claude Code 本身运行
  // ============================================
  try {
    const { callDaemon } = await import("./lib/daemon-client.mjs");

    // 从 groups 目录获取当前活跃的 group_id
    const groupId = resolveGroupId();
    if (groupId) {
      const resp = await callDaemon("actor_start", {
        group_id: groupId,
        actor_id: agentId,
        by: "system",
      });
      if (resp?.ok) {
        console.log(`[GhostCode] SubagentStart: Actor ${agentId} 已向 Daemon 注册`);
      } else {
        console.error(`[GhostCode] SubagentStart: Daemon actor_start 失败:`, resp?.error?.message || "未知错误");
      }
    }
  } catch (err) {
    // Daemon IPC 失败不阻断子 Agent 启动（Hook exit 0 规范）
    console.error("[GhostCode] SubagentStart: Daemon IPC 异常:", err.message);
  }
}

// ============================================
// 入口：执行主逻辑
// exit 0 策略：记录失败不阻断子 Agent 启动
// ============================================
main().catch((err) => {
  console.error("[GhostCode] hook-subagent-start 异常:", err);
  process.exit(0);
});
