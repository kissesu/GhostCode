/**
 * @file AgentGraph.tsx
 * @description Agent 状态面板组件，列表形式展示各 Actor 的运行状态
 *
 * 业务逻辑说明：
 * 1. 以列表形式展示所有 Agent，避免拓扑图复杂度（YAGNI 原则）
 * 2. 每个 Agent 显示：ID、Runtime 类型、状态指示（在线/离线/未知）、最后活跃时间、消息数
 * 3. 按状态排序：active > unknown > stopped
 *
 * @author Atlas.oi
 * @date 2026-03-03
 */

import type { AgentStatusView } from '../api/client';

/** AgentGraph 组件属性 */
interface AgentGraphProps {
  /** Agent 状态视图数组 */
  agents: AgentStatusView[];
}

/**
 * 状态排序权重（数值越小排序越靠前）
 */
const STATUS_ORDER: Record<string, number> = {
  active: 0,
  unknown: 1,
  stopped: 2,
};

/**
 * 判断字符串是否为纯十六进制 ID（如 Claude Code 子 Agent 的会话 ID）
 *
 * @param id - 待检测的字符串
 * @returns true 表示是 hex ID
 */
function isHexId(id: string): boolean {
  return /^[0-9a-f]{10,}$/i.test(id);
}

/**
 * 判断 Agent 是否为"幽灵 Agent"——缺少有意义元数据的已停止 Agent
 *
 * 业务逻辑说明：
 * 当 SubagentStart Hook 未正确触发时，账本中只有 actor.stop 事件，
 * 没有 actor.start 事件。这些 Agent 的 display_name 和 agent_type 完全缺失，
 * 展示一堆 "Agent-xxxx subagent stopped" 只会让用户困惑，不如直接隐藏。
 *
 * 过滤规则（同时满足以下三条才判定为幽灵 Agent）：
 * 1. 已停止（status !== 'active'）
 * 2. 没有有意义的显示名称（display_name 缺失）
 * 3. actor_id 是 hex ID（无法从中推断出任何业务含义）
 *
 * @param agent - Agent 状态视图
 * @returns true 表示应隐藏
 */
function isGhostAgent(agent: AgentStatusView): boolean {
  return agent.status !== 'active'
    && !agent.display_name
    && !agent.agent_type
    && isHexId(agent.actor_id);
}

/**
 * 生成 Agent 的友好显示名称
 *
 * 业务逻辑说明：
 * 1. 优先使用 display_name（由 SubagentStart Hook 从 agent_type 生成）
 * 2. 若 display_name 缺失且 actor_id 为 hex ID，截断显示为 "Agent-xxxx"
 * 3. 否则直接显示 actor_id
 *
 * @param agent - Agent 状态视图
 * @returns 友好的显示名称
 */
function getDisplayName(agent: AgentStatusView): string {
  if (agent.display_name) return agent.display_name;
  if (isHexId(agent.actor_id)) {
    return `Agent-${agent.actor_id.slice(0, 6)}`;
  }
  return agent.actor_id;
}

/**
 * 获取 Agent 类型标签的显示文本
 *
 * 业务逻辑说明：
 * 1. 优先使用 agent_type（如 "feature-dev:code-reviewer"）
 * 2. 若 runtime 不是 "custom"，使用 runtime（如 "claude"、"codex"）
 * 3. 若 runtime 为 "custom" 且 actor_id 为 hex ID，显示 "subagent"
 * 4. 否则显示 runtime 原始值
 *
 * @param agent - Agent 状态视图
 * @returns 类型标签文本
 */
function getTypeLabel(agent: AgentStatusView): string {
  if (agent.agent_type) return agent.agent_type;
  if (agent.runtime !== 'custom') return agent.runtime;
  if (isHexId(agent.actor_id)) return 'subagent';
  return agent.runtime;
}

/**
 * 获取 Runtime 类型的显示颜色
 *
 * @param runtime - Runtime 类型字符串
 * @returns CSS 颜色值
 */
function getRuntimeColor(runtime: string): string {
  switch (runtime.toLowerCase()) {
    case 'claude':
      return 'var(--accent-purple)';
    case 'codex':
      return 'var(--accent-blue)';
    case 'gemini':
      return 'var(--accent-green)';
    default:
      return 'var(--text-secondary)';
  }
}

/**
 * 格式化最后活跃时间
 *
 * @param lastSeen - ISO 8601 时间戳（可选）
 * @returns 人类可读的时间字符串
 */
function formatLastSeen(lastSeen: string | null): string {
  if (!lastSeen) return '从未';

  const then = new Date(lastSeen).getTime();
  if (isNaN(then)) return lastSeen;

  const diffMs = Date.now() - then;
  const diffSec = Math.floor(diffMs / 1000);

  if (diffSec < 60) return `${diffSec}s 前`;
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin}m 前`;
  const diffHour = Math.floor(diffMin / 60);
  return `${diffHour}h 前`;
}

/**
 * 单个 Agent 状态卡片
 */
function AgentCard({ agent }: { agent: AgentStatusView }) {
  // 友好显示名称：优先 display_name → hex ID 截断 → 原始 actor_id
  const displayName = getDisplayName(agent);
  // 类型标签：优先 agent_type → 有意义的 runtime → "subagent"
  const typeLabel = getTypeLabel(agent);
  // 颜色基于 runtime 推断（claude=紫、codex=蓝、gemini=绿），custom 用默认色
  const runtimeColor = getRuntimeColor(agent.runtime);

  return (
    <div
      className="card p-3 flex flex-col gap-2"
      data-testid="agent-card"
    >
      {/* 头部：状态指示 + 友好显示名称 */}
      <div className="flex items-center gap-2">
        <span className={`status-dot ${agent.status}`} data-testid="status-dot" />
        <span
          className="text-sm font-mono font-medium truncate flex-1"
          style={{ color: 'var(--text-primary)' }}
          title={agent.actor_id}
        >
          {displayName}
        </span>
      </div>

      {/* Runtime / Agent 类型标签 + 状态文字 */}
      <div className="flex items-center gap-2">
        <span
          className="text-xs px-1.5 py-0.5 rounded font-mono"
          style={{
            color: runtimeColor,
            backgroundColor: `${runtimeColor}22`,
            border: `1px solid ${runtimeColor}44`,
          }}
        >
          {typeLabel}
        </span>
        <span className="text-xs" style={{ color: 'var(--text-muted)' }}>
          {agent.status}
        </span>
      </div>

      {/* 最后活跃时间 */}
      <div className="text-xs" style={{ color: 'var(--text-muted)' }}>
        最后活跃：{formatLastSeen(agent.last_seen)}
      </div>
    </div>
  );
}

/**
 * Agent 状态面板组件
 *
 * @param agents - Agent 状态视图数组
 */
export function AgentGraph({ agents }: AgentGraphProps) {
  // ============================================
  // 过滤幽灵 Agent：缺少元数据的已停止 hex ID Agent
  // 这些 Agent 因 SubagentStart Hook 未触发而缺少 display_name/agent_type，
  // 展示无意义的 "Agent-xxxx subagent stopped" 只会让用户困惑
  // ============================================
  const visibleAgents = agents.filter((a) => !isGhostAgent(a));

  if (visibleAgents.length === 0) {
    return (
      <div
        className="flex items-center justify-center h-32 text-sm"
        style={{ color: 'var(--text-muted)' }}
      >
        暂无 Agent
      </div>
    );
  }

  // 排序规则：1) 状态优先（active > unknown > stopped）2) 同状态按最后活跃时间倒序
  const sortedAgents = [...visibleAgents].sort((a, b) => {
    const orderA = STATUS_ORDER[a.status] ?? 3;
    const orderB = STATUS_ORDER[b.status] ?? 3;
    if (orderA !== orderB) return orderA - orderB;
    // C4 修复：使用数值比较替代字符串比较
    // 字符串 localeCompare 对 ISO 8601 时间戳在跨时区或不同格式下不可靠
    const tsA = a.last_seen ? new Date(a.last_seen).getTime() : 0;
    const tsB = b.last_seen ? new Date(b.last_seen).getTime() : 0;
    return tsB - tsA;
  });

  const activeCount = visibleAgents.filter((a) => a.status === 'active').length;
  // 被隐藏的幽灵 Agent 数量（仅当存在时显示提示）
  const hiddenCount = agents.length - visibleAgents.length;

  return (
    <div className="flex flex-col gap-3" data-testid="agent-graph">
      {/* 统计摘要 */}
      <div
        className="text-xs px-1"
        style={{ color: 'var(--text-muted)' }}
      >
        {activeCount} / {visibleAgents.length} 个 Agent 在线
        {hiddenCount > 0 && (
          <span title="缺少元数据的已停止 Agent（SubagentStart Hook 未触发）">
            {` (+${hiddenCount} 已回收)`}
          </span>
        )}
      </div>

      {/* Agent 列表 */}
      <div className="flex flex-col gap-2">
        {sortedAgents.map((agent) => (
          <AgentCard key={agent.actor_id} agent={agent} />
        ))}
      </div>
    </div>
  );
}
