/**
 * @file RouteTimelineEvent.tsx
 * @description Route 事件复合状态组件，展示 LLM 路由调用的生命周期
 *
 * 业务逻辑说明：
 * 1. route.start 显示为"进行中"状态（脉冲动画 + 后端名称 + 任务摘要）
 * 2. route.complete 显示为"已完成"状态（绿色 + 耗时 + 输出摘要）
 * 3. route.error 显示为"错误"状态（红色 + 错误信息）
 *
 * @author Atlas.oi
 * @date 2026-03-16
 */

import type { LedgerTimelineItem } from '../../api/client';

/** Route 事件组件属性 */
interface RouteTimelineEventProps {
  item: LedgerTimelineItem;
}

/**
 * 解析 route 事件的 data_summary JSON
 *
 * @param rawSummary - 原始 JSON 字符串
 * @returns 解析后的键值对，解析失败返回空对象
 */
function parseRouteData(rawSummary: string): Record<string, unknown> {
  try {
    return JSON.parse(rawSummary) as Record<string, unknown>;
  } catch {
    return {};
  }
}

/**
 * 根据 kind 判断 route 事件的状态类型
 *
 * @param kind - 事件类型字符串
 * @returns 状态枚举：running / completed / error
 */
function getRouteStatus(kind: string): 'running' | 'completed' | 'error' {
  if (kind === 'route.complete') return 'completed';
  if (kind === 'route.error') return 'error';
  return 'running';
}

/**
 * 计算相对时间字符串
 *
 * @param ts - ISO 8601 时间戳字符串
 * @returns 人类可读的相对时间（如 "3 分钟前"）
 */
function formatRelativeTime(ts: string): string {
  const now = Date.now();
  const then = new Date(ts).getTime();
  if (isNaN(then)) return ts;
  const diffSec = Math.floor((now - then) / 1000);
  if (diffSec < 60) return `${diffSec} 秒前`;
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin} 分钟前`;
  const diffHour = Math.floor(diffMin / 60);
  if (diffHour < 24) return `${diffHour} 小时前`;
  return `${Math.floor(diffHour / 24)} 天前`;
}

/**
 * Route 事件复合状态组件
 *
 * 根据 route.start / route.complete / route.error 三种状态
 * 展示不同的视觉样式和数据字段。
 */
export function RouteTimelineEvent({ item }: RouteTimelineEventProps) {
  const data = parseRouteData(item.data_summary);
  const status = getRouteStatus(item.kind);

  // 从 data_summary 中提取各字段
  const backend = (data.backend as string) || 'unknown';
  const taskSummary = data.task_summary as string | undefined;
  const durationMs = data.duration_ms as number | undefined;
  const outputSummary = data.output_summary as string | undefined;
  const errorMessage = data.error_message as string | undefined;

  // 根据状态选取颜色变量
  const statusColor =
    status === 'error'
      ? 'var(--accent-red)'
      : status === 'completed'
        ? 'var(--accent-green)'
        : 'var(--accent-purple)';

  return (
    <div
      className="flex gap-3 py-2 px-3 hover:bg-[var(--bg-hover)] rounded transition-colors"
      data-testid="route-timeline-event"
    >
      {/* 左侧时间轴竖线 + 状态指示圆点 */}
      <div className="flex flex-col items-center pt-1">
        <div className="relative shrink-0">
          {/* 状态圆点 */}
          <div
            className="w-2.5 h-2.5 rounded-full"
            style={{ backgroundColor: statusColor }}
          />
          {/* 进行中（route.start）时叠加脉冲扩散动画，表示异步等待 */}
          {status === 'running' && (
            <div
              className="absolute inset-0 w-2.5 h-2.5 rounded-full animate-ping"
              style={{ backgroundColor: statusColor, opacity: 0.4 }}
            />
          )}
        </div>
        {/* 时间轴竖线 */}
        <div className="w-px flex-1 mt-1" style={{ backgroundColor: 'var(--border-subtle)' }} />
      </div>

      {/* 右侧事件内容区域 */}
      <div className="flex-1 min-w-0 pb-2">
        {/* 事件头部：kind 标签 + 后端标签 + 耗时 + 时间戳 */}
        <div className="flex items-center gap-2 flex-wrap">
          {/* Route 状态标签（颜色随状态变化） */}
          <span
            className="text-xs px-1.5 py-0.5 rounded font-mono font-medium shrink-0"
            style={{
              color: statusColor,
              backgroundColor: `${statusColor}22`,
              border: `1px solid ${statusColor}44`,
            }}
          >
            {item.kind}
          </span>

          {/* 后端名称标签（蓝色，标识 LLM 提供方） */}
          <span
            className="text-xs px-1.5 py-0.5 rounded font-mono shrink-0"
            style={{
              color: 'var(--accent-blue)',
              backgroundColor: 'var(--accent-blue)22',
              border: '1px solid var(--accent-blue)44',
            }}
          >
            {backend}
          </span>

          {/* 耗时（仅 complete / error 时展示） */}
          {durationMs != null && (
            <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
              {(durationMs / 1000).toFixed(1)}s
            </span>
          )}

          {/* 相对时间戳，靠右对齐 */}
          <span className="text-xs ml-auto shrink-0" style={{ color: 'var(--text-muted)' }}>
            {formatRelativeTime(item.ts)}
          </span>
        </div>

        {/* 任务摘要（route.start 时展示，超长截断） */}
        {taskSummary && (
          <div
            className="mt-1 text-xs truncate"
            style={{ color: 'var(--text-muted)' }}
            title={taskSummary}
          >
            {taskSummary.length > 120 ? taskSummary.slice(0, 120) + '...' : taskSummary}
          </div>
        )}

        {/* 输出摘要（仅 route.complete 时展示） */}
        {status === 'completed' && outputSummary && (
          <div
            className="mt-1 text-xs truncate"
            style={{ color: 'var(--accent-green)' }}
            title={outputSummary}
          >
            {outputSummary.length > 100 ? outputSummary.slice(0, 100) + '...' : outputSummary}
          </div>
        )}

        {/* 错误信息（仅 route.error 时展示） */}
        {status === 'error' && errorMessage && (
          <div
            className="mt-1 text-xs"
            style={{ color: 'var(--accent-red)' }}
            title={errorMessage}
          >
            {errorMessage.length > 150 ? errorMessage.slice(0, 150) + '...' : errorMessage}
          </div>
        )}
      </div>
    </div>
  );
}
