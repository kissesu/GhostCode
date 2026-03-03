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
  const runtimeColor = getRuntimeColor(agent.runtime);

  return (
    <div
      className="card p-3 flex flex-col gap-2"
      data-testid="agent-card"
    >
      {/* 头部：状态指示 + Actor ID */}
      <div className="flex items-center gap-2">
        <span className={`status-dot ${agent.status}`} data-testid="status-dot" />
        <span
          className="text-sm font-mono font-medium truncate flex-1"
          style={{ color: 'var(--text-primary)' }}
          title={agent.actor_id}
        >
          {agent.actor_id}
        </span>
      </div>

      {/* Runtime 类型标签 */}
      <div className="flex items-center gap-2">
        <span
          className="text-xs px-1.5 py-0.5 rounded font-mono"
          style={{
            color: runtimeColor,
            backgroundColor: `${runtimeColor}22`,
            border: `1px solid ${runtimeColor}44`,
          }}
        >
          {agent.runtime}
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
  if (agents.length === 0) {
    return (
      <div
        className="flex items-center justify-center h-32 text-sm"
        style={{ color: 'var(--text-muted)' }}
      >
        暂无 Agent
      </div>
    );
  }

  // 按状态排序：active > unknown > stopped
  const sortedAgents = [...agents].sort((a, b) => {
    const orderA = STATUS_ORDER[a.status] ?? 3;
    const orderB = STATUS_ORDER[b.status] ?? 3;
    return orderA - orderB;
  });

  const activeCount = agents.filter((a) => a.status === 'active').length;

  return (
    <div className="flex flex-col gap-3" data-testid="agent-graph">
      {/* 统计摘要 */}
      <div
        className="text-xs px-1"
        style={{ color: 'var(--text-muted)' }}
      >
        {activeCount} / {agents.length} 个 Agent 在线
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
