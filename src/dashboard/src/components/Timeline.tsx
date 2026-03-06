/**
 * @file Timeline.tsx
 * @description 纵向事件时间轴组件，展示账本事件流
 *
 * 业务逻辑说明：
 * 1. 前端按时间戳倒序排列，最新事件在顶部
 * 2. 新事件到达时自动滚动到顶部
 * 3. 每个事件显示：kind 标签（彩色）、by（Actor）、ts（相对时间）、data_summary
 *
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { useEffect, useRef } from 'react';
import type { LedgerTimelineItem } from '../api/client';

/** Timeline 组件属性 */
interface TimelineProps {
  /** 事件列表（渲染时自动按时间倒序排列，最新在顶部） */
  items: LedgerTimelineItem[];
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
  const diffMs = now - then;

  if (isNaN(then)) return ts;

  const diffSec = Math.floor(diffMs / 1000);
  if (diffSec < 60) return `${diffSec} 秒前`;

  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin} 分钟前`;

  const diffHour = Math.floor(diffMin / 60);
  if (diffHour < 24) return `${diffHour} 小时前`;

  const diffDay = Math.floor(diffHour / 24);
  return `${diffDay} 天前`;
}

/**
 * 根据事件 kind 返回对应的颜色 CSS 变量
 *
 * @param kind - 事件类型字符串
 * @returns CSS 颜色值
 */
function getKindColor(kind: string): string {
  const lowerKind = kind.toLowerCase();
  if (lowerKind.includes('error') || lowerKind.includes('fail')) {
    return 'var(--accent-red)';
  }
  if (lowerKind.includes('join') || lowerKind.includes('start') || lowerKind.includes('success')) {
    return 'var(--accent-green)';
  }
  if (lowerKind.includes('message') || lowerKind.includes('send')) {
    return 'var(--accent-blue)';
  }
  if (lowerKind.includes('skill') || lowerKind.includes('learn')) {
    return 'var(--accent-purple)';
  }
  if (lowerKind.includes('left') || lowerKind.includes('stop')) {
    return 'var(--accent-yellow)';
  }
  return 'var(--text-secondary)';
}

/**
 * 单条时间线事件条目
 */
function TimelineItem({ item }: { item: LedgerTimelineItem }) {
  const kindColor = getKindColor(item.kind);

  return (
    <div
      className="flex gap-3 py-2 px-3 hover:bg-[var(--bg-hover)] rounded transition-colors"
      data-testid="timeline-item"
    >
      {/* 左侧时间轴竖线 + 圆点 */}
      <div className="flex flex-col items-center pt-1">
        <div
          className="w-2 h-2 rounded-full shrink-0"
          style={{ backgroundColor: kindColor }}
        />
        <div className="w-px flex-1 mt-1" style={{ backgroundColor: 'var(--border-subtle)' }} />
      </div>

      {/* 右侧事件内容 */}
      <div className="flex-1 min-w-0 pb-2">
        {/* 事件头部：kind 标签 + Actor + 时间 */}
        <div className="flex items-center gap-2 flex-wrap">
          <span
            className="text-xs px-1.5 py-0.5 rounded font-mono font-medium shrink-0"
            style={{
              color: kindColor,
              backgroundColor: `${kindColor}22`,
              border: `1px solid ${kindColor}44`,
            }}
          >
            {item.kind}
          </span>
          <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
            by <span style={{ color: 'var(--accent-blue)' }}>{item.by}</span>
          </span>
          <span className="text-xs ml-auto shrink-0" style={{ color: 'var(--text-muted)' }}>
            {formatRelativeTime(item.ts)}
          </span>
        </div>

        {/* 事件数据摘要 */}
        {item.data_summary && (
          <div
            className="mt-1 text-xs font-mono truncate"
            style={{ color: 'var(--text-muted)' }}
            title={item.data_summary}
          >
            {item.data_summary}
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * 纵向事件时间轴组件
 *
 * @param items - 事件列表
 */
export function Timeline({ items }: TimelineProps) {
  // C3 修复：用 containerRef 绑定到外层滚动容器，替代 topRef + scrollIntoView
  // scrollIntoView 会影响页面级滚动，containerRef.scrollTo 仅控制 Timeline 内部滚动
  const containerRef = useRef<HTMLDivElement>(null);

  // 新事件到达时，仅在用户接近顶部（scrollTop < 100px）时自动滚动
  // 避免用户正在浏览历史事件时被强制拉回顶部
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const timer = setTimeout(() => {
      if (container.scrollTop < 100) {
        container.scrollTo({ top: 0, behavior: 'smooth' });
      }
    }, 100);
    return () => clearTimeout(timer);
  }, [items.length]);

  if (items.length === 0) {
    return (
      <div
        className="flex items-center justify-center h-full text-sm"
        style={{ color: 'var(--text-muted)' }}
      >
        暂无事件
      </div>
    );
  }

  // 按时间戳倒序排列（最新事件在顶部）
  // 后端 + SSE 层已保证倒序，此处作为防御性排序保底
  // W5-review：使用数值比较替代 localeCompare，与 AgentGraph 的 C4 修复保持一致
  // localeCompare 对含时区偏移的 ISO 8601 时间戳不可靠
  const sortedItems = [...items].sort((a, b) => {
    const tsA = new Date(a.ts).getTime();
    const tsB = new Date(b.ts).getTime();
    return tsB - tsA;
  });

  return (
    <div ref={containerRef} className="flex flex-col h-full overflow-y-auto" data-testid="timeline">
      {sortedItems.map((item) => (
        <TimelineItem key={item.id} item={item} />
      ))}
    </div>
  );
}
