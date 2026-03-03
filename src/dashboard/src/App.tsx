/**
 * @file App.tsx
 * @description GhostCode Dashboard 主应用组件，三栏布局
 *
 * 业务逻辑说明：
 * 1. 三栏布局：左侧 Agent 列表（宽度固定）、中间 Timeline（弹性伸展）、右侧 Skill 面板（宽度固定）
 * 2. 顶部状态栏显示：Group ID、SSE 连接状态、事件总数、刷新按钮
 * 3. 通过 useDashboard Hook 获取所有数据，避免组件间直接通信
 *
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { AgentGraph } from './components/AgentGraph';
import { SkillPanel } from './components/SkillPanel';
import { Timeline } from './components/Timeline';
import { useDashboard } from './hooks/useDashboard';

// 默认监控的 Group ID（实际部署时从 URL 参数或配置获取）
const DEFAULT_GROUP_ID = 'default';

/**
 * 从 URL 参数获取 Group ID
 * 支持：?group=xxx 格式
 *
 * @returns Group ID 字符串
 */
function getGroupId(): string {
  if (typeof window !== 'undefined') {
    const params = new URLSearchParams(window.location.search);
    return params.get('group') ?? DEFAULT_GROUP_ID;
  }
  return DEFAULT_GROUP_ID;
}

/**
 * 顶部状态栏组件
 */
function StatusBar({
  groupId,
  sseConnected,
  totalEvents,
  loading,
  onRefresh,
}: {
  groupId: string;
  sseConnected: boolean;
  totalEvents: number;
  loading: boolean;
  onRefresh: () => void;
}) {
  return (
    <header
      className="flex items-center gap-4 px-4 py-2 border-b shrink-0"
      style={{
        backgroundColor: 'var(--bg-card)',
        borderColor: 'var(--border-default)',
      }}
    >
      {/* 项目名称 */}
      <span
        className="text-sm font-bold font-mono tracking-wide"
        style={{ color: 'var(--text-primary)' }}
      >
        GhostCode Dashboard
      </span>

      {/* 分隔符 */}
      <span style={{ color: 'var(--border-default)' }}>|</span>

      {/* Group ID */}
      <span className="text-xs font-mono" style={{ color: 'var(--text-secondary)' }}>
        group: <span style={{ color: 'var(--accent-blue)' }}>{groupId}</span>
      </span>

      {/* 分隔符 */}
      <span style={{ color: 'var(--border-default)' }}>|</span>

      {/* SSE 连接状态 */}
      <div className="flex items-center gap-1.5">
        <span className={`status-dot ${sseConnected ? 'active' : 'stopped'}`} />
        <span className="text-xs" style={{ color: 'var(--text-muted)' }}>
          {sseConnected ? '实时连接' : '离线'}
        </span>
      </div>

      {/* 事件总数 */}
      <span className="text-xs" style={{ color: 'var(--text-muted)' }}>
        {totalEvents.toLocaleString()} 个事件
      </span>

      {/* 弹性空白，将刷新按钮推到右侧 */}
      <div className="flex-1" />

      {/* 刷新按钮 */}
      <button
        className="text-xs px-3 py-1 rounded transition-colors disabled:opacity-50"
        style={{
          color: 'var(--text-secondary)',
          backgroundColor: 'var(--border-subtle)',
          border: '1px solid var(--border-default)',
        }}
        onClick={onRefresh}
        disabled={loading}
      >
        {loading ? '加载中...' : '刷新'}
      </button>
    </header>
  );
}

/**
 * 错误提示横幅
 */
function ErrorBanner({ message }: { message: string }) {
  return (
    <div
      className="px-4 py-2 text-xs shrink-0"
      style={{
        backgroundColor: 'var(--accent-red)22',
        borderBottom: '1px solid var(--accent-red)44',
        color: 'var(--accent-red)',
      }}
    >
      错误：{message}
    </div>
  );
}

/**
 * 加载骨架屏
 */
function LoadingSkeleton() {
  return (
    <div
      className="flex items-center justify-center flex-1 text-sm"
      style={{ color: 'var(--text-muted)' }}
    >
      正在加载 Dashboard 数据...
    </div>
  );
}

/**
 * GhostCode Dashboard 主应用组件
 */
export default function App() {
  const groupId = getGroupId();

  const {
    snapshot,
    skills,
    loading,
    error,
    sseConnected,
    handlePromoteSkill,
    refresh,
  } = useDashboard(groupId);

  return (
    <div
      className="flex flex-col h-screen"
      style={{ backgroundColor: 'var(--bg-primary)' }}
    >
      {/* 顶部状态栏 */}
      <StatusBar
        groupId={groupId}
        sseConnected={sseConnected}
        totalEvents={snapshot?.total_events ?? 0}
        loading={loading}
        onRefresh={refresh}
      />

      {/* 错误横幅（有错误时显示） */}
      {error && <ErrorBanner message={error} />}

      {/* 主内容区域 */}
      {loading && !snapshot ? (
        <LoadingSkeleton />
      ) : (
        <div className="flex flex-1 min-h-0 gap-0">
          {/* ============================================
              左栏：Agent 状态面板（固定宽度 240px）
              ============================================ */}
          <aside
            className="w-60 shrink-0 flex flex-col border-r overflow-y-auto"
            style={{
              borderColor: 'var(--border-default)',
              backgroundColor: 'var(--bg-card)',
            }}
          >
            <div
              className="px-3 py-2 text-xs font-semibold border-b shrink-0"
              style={{
                color: 'var(--text-secondary)',
                borderColor: 'var(--border-subtle)',
              }}
            >
              Agents
            </div>
            <div className="p-2 flex-1 overflow-y-auto">
              <AgentGraph agents={snapshot?.agents ?? []} />
            </div>
          </aside>

          {/* ============================================
              中栏：事件时间轴（弹性伸展，占剩余空间）
              ============================================ */}
          <main className="flex-1 flex flex-col min-w-0 overflow-hidden">
            <div
              className="px-3 py-2 text-xs font-semibold border-b shrink-0"
              style={{
                color: 'var(--text-secondary)',
                borderColor: 'var(--border-default)',
                backgroundColor: 'var(--bg-card)',
              }}
            >
              Timeline
            </div>
            <div className="flex-1 overflow-hidden">
              <Timeline items={snapshot?.recent_timeline ?? []} />
            </div>
          </main>

          {/* ============================================
              右栏：Skill 候选面板（固定宽度 256px）
              ============================================ */}
          <aside
            className="w-64 shrink-0 flex flex-col border-l overflow-y-auto"
            style={{
              borderColor: 'var(--border-default)',
              backgroundColor: 'var(--bg-card)',
            }}
          >
            <div
              className="px-3 py-2 text-xs font-semibold border-b shrink-0"
              style={{
                color: 'var(--text-secondary)',
                borderColor: 'var(--border-subtle)',
              }}
            >
              Skills
            </div>
            <div className="p-2 flex-1 overflow-y-auto">
              <SkillPanel skills={skills} onPromote={handlePromoteSkill} />
            </div>
          </aside>
        </div>
      )}
    </div>
  );
}
