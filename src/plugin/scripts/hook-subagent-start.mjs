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
// 显示名称生成
// ============================================

/**
 * 根据 agent_type 生成人类可读的显示名称
 *
 * 转换规则：
 * - "feature-dev:code-reviewer" -> "Code Reviewer"（取冒号后部分）
 * - "general-purpose" -> "General Purpose"（直接转换）
 * - kebab-case 和 snake_case 转换为 Title Case
 *
 * @param {string|null|undefined} agentType - Claude Code 传入的 agent_type 标识
 * @returns {string|null} 人类可读名称，无法生成时返回 null
 */
/**
 * 将单词首字母大写
 *
 * @param {string} word - 待转换的单词
 * @returns {string} 首字母大写的单词
 */
function capitalizeWord(word) {
  return word.charAt(0).toUpperCase() + word.slice(1);
}

function generateDisplayName(agentType) {
  if (!agentType || agentType.trim() === '') return null;
  // W3 修复：取冒号后部分，若为空则回退到冒号前部分
  // 例如 "feature-dev:" → pop() 返回空字符串 → 回退到 "feature-dev"
  let part = agentType;
  if (agentType.includes(':')) {
    const segments = agentType.split(':');
    const last = segments.pop();
    part = (last && last.trim() !== '') ? last : segments[0] || agentType;
  }
  // kebab-case / snake_case 转 Title Case，过滤空元素防止多余空格
  const result = part.split(/[-_]/).filter(Boolean).map(capitalizeWord).join(' ');
  return result || null;
}

// ============================================
// 主逻辑
// ============================================

/**
 * SubagentStart Hook 主函数
 *
 * 业务逻辑说明：
 * 1. 从 stdin 读取 Claude Code 传入的事件 JSON
 * 2. 提取子 Agent ID、会话 ID、agent_type 并生成 display_name
 * 3. 将子 Agent 记录追加到 subagents.json 状态文件
 * 4. 向 Daemon 注册 Actor（IPC actor_start），携带 display_name 和 agent_type
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
  // W5 修复：无 agent_id 时记录错误并提前退出，不写入无效数据
  if (!inner?.agent_id && !inner?.agentId) {
    console.error("[GhostCode] SubagentStart: stdin 中未找到 agent_id 字段，跳过注册");
    process.exit(0);
  }

  // 提取 agent_type 并生成人类可读的 display_name
  const agentType = inner?.agent_type || inner?.agentType || null;
  const displayName = generateDisplayName(agentType);

  // ============================================
  // 第三步：记录子 Agent 到状态文件
  // 追加到已有的 agents 记录，不覆盖其他活跃 Agent
  // ============================================
  const state = readSubagentsState();
  state.agents[agentId] = {
    startedAt: new Date().toISOString(),
    sessionId,
    agentType,
    displayName,
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
      // 仅在有值时附加可选字段，避免发送 null（与 Rust 侧惯例一致）
      const ipcArgs = { group_id: groupId, actor_id: agentId, by: "system" };
      if (displayName) ipcArgs.display_name = displayName;
      if (agentType) ipcArgs.agent_type = agentType;
      const resp = await callDaemon("actor_start", ipcArgs);
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
