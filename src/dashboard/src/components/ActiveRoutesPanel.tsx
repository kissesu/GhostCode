/**
 * @file ActiveRoutesPanel.tsx
 * @description 实时路由状态面板，展示当前所有进行中的 LLM 调用
 *
 * 业务逻辑说明：
 * 1. 列表展示所有 activeRoutes（status === 'running' 的 RouteEvent）
 * 2. 每项显示：后端名称、任务摘要、动态计时器
 * 3. 空状态提示"当前无活动的 LLM 调用"
 *
 * @author Atlas.oi
 * @date 2026-03-16
 */

import { useEffect, useState } from 'react';
import type { RouteEvent } from '../api/client';

/** ActiveRoutesPanel 组件属性 */
interface ActiveRoutesPanelProps {
  /** 活动中的 Route 调用列表 */
  activeRoutes: RouteEvent[];
}

/**
 * 动态计时器组件
 *
 * 从 startTs 开始每 100ms 更新显示经过的时间
 *
 * @param startTs - 路由开始时间戳（ISO 8601）
 */
function ElapsedTimer({ startTs }: { startTs: string }) {
  const [elapsed, setElapsed] = useState('0.0s');

  useEffect(() => {
    const startTime = new Date(startTs).getTime();

    const update = () => {
      const now = Date.now();
      const diffSec = (now - startTime) / 1000;
      setElapsed(`${diffSec.toFixed(1)}s`);
    };

    update();
    const timer = setInterval(update, 100);
    return () => clearInterval(timer);
  }, [startTs]);

  return (
    <span className="text-xs font-mono tabular-nums" style={{ color: 'var(--accent-purple)' }}>
      {elapsed}
    </span>
  );
}

/**
 * 获取后端对应的颜色
 *
 * @param backend - 后端名称（claude/codex/gemini/其他）
 * @returns CSS 颜色值
 */
function getBackendColor(backend: string): string {
  switch (backend.toLowerCase()) {
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
 * 单个活动路由条目
 *
 * 展示后端标签、脉冲指示器、计时器以及任务摘要
 *
 * @param route - RouteEvent 数据
 */
function ActiveRouteItem({ route }: { route: RouteEvent }) {
  const backendColor = getBackendColor(route.backend);

  return (
    <div
      className="flex flex-col gap-1 p-2 rounded transition-all"
      style={{ backgroundColor: 'var(--bg-hover)' }}
    >
      {/* 头部：脉冲指示器 + 后端标签 + 计时器 */}
      <div className="flex items-center gap-2">
        {/* 脉冲指示器 */}
        <div className="relative shrink-0">
          <div
            className="w-2 h-2 rounded-full"
            style={{ backgroundColor: 'var(--accent-purple)' }}
          />
          <div
            className="absolute inset-0 w-2 h-2 rounded-full animate-ping"
            style={{ backgroundColor: 'var(--accent-purple)', opacity: 0.4 }}
          />
        </div>

        {/* 后端名称标签 */}
        <span
          className="text-xs px-1.5 py-0.5 rounded font-mono font-medium"
          style={{
            color: backendColor,
            backgroundColor: `${backendColor}22`,
            border: `1px solid ${backendColor}44`,
          }}
        >
          {route.backend}
        </span>

        {/* 动态计时器，靠右显示 */}
        <div className="ml-auto">
          <ElapsedTimer startTs={route.startTs} />
        </div>
      </div>

      {/* 任务摘要（超过 60 字符截断） */}
      {route.taskSummary && (
        <div
          className="text-xs truncate pl-4"
          style={{ color: 'var(--text-muted)' }}
          title={route.taskSummary}
        >
          {route.taskSummary.length > 60 ? route.taskSummary.slice(0, 60) + '...' : route.taskSummary}
        </div>
      )}
    </div>
  );
}

/**
 * 实时路由状态面板
 *
 * 业务逻辑说明：
 * 1. activeRoutes 为空时显示"当前无活动的 LLM 调用"空状态
 * 2. 每个路由条目包含脉冲动画、后端标签、动态计时器、任务摘要
 *
 * @param activeRoutes - 活动中的 Route 调用列表
 */
export function ActiveRoutesPanel({ activeRoutes }: ActiveRoutesPanelProps) {
  // 空状态：无活动路由时显示提示文字
  if (activeRoutes.length === 0) {
    return (
      <div
        className="flex items-center justify-center py-4 text-xs"
        style={{ color: 'var(--text-muted)' }}
      >
        当前无活动的 LLM 调用
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2" data-testid="active-routes-panel">
      {activeRoutes.map((route) => (
        <ActiveRouteItem key={route.correlationId} route={route} />
      ))}
    </div>
  );
}
