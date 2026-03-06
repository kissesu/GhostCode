/**
 * @file useDashboard.ts
 * @description Dashboard 数据聚合 Hook，整合 REST 快照拉取与 SSE 增量更新
 *
 * 业务逻辑说明：
 * 1. 初始化时通过 REST 拉取 DashboardSnapshot（含最近 20 条时间线和 Agent 列表）
 * 2. 通过 SSE 订阅实时事件流，将新事件追加到 timeline
 * 3. 暴露加载状态、错误信息和聚合后的 snapshot 数据
 *
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import type {
  AgentStatusView,
  DashboardSnapshot,
  LearnedSkill,
  LedgerTimelineItem,
} from '../api/client';
import { fetchDashboard, fetchSkills, promoteSkill } from '../api/client';
import { useSSE } from './useSSE';

/** Dashboard Hook 返回值 */
export interface UseDashboardResult {
  /** 聚合后的快照数据（含实时更新的 timeline） */
  snapshot: DashboardSnapshot | null;
  /** 已学习的 Skill 列表 */
  skills: LearnedSkill[];
  /** 是否正在初始加载 */
  loading: boolean;
  /** 错误信息（null 表示无错误） */
  error: string | null;
  /** SSE 是否已连接 */
  sseConnected: boolean;
  /** 提升 Skill 到正式库 */
  handlePromoteSkill: (skillId: string) => Promise<void>;
  /** 手动刷新快照 */
  refresh: () => void;
}

/**
 * Dashboard 数据聚合 Hook
 *
 * 业务逻辑说明：
 * 1. 拉取初始快照，作为 timeline 的基线数据
 * 2. SSE 新事件到达时，合并到 timeline（去重：按 id 过滤）
 * 3. Agent 列表只从 REST 刷新（不通过 SSE 更新状态）
 *
 * @param groupId - 要监控的 Group ID
 * @param baseUrl - 后端基础 URL（可选）
 * @returns Dashboard 聚合状态和操作函数
 */
export function useDashboard(groupId: string | null, baseUrl = ''): UseDashboardResult {
  const [snapshot, setSnapshot] = useState<DashboardSnapshot | null>(null);
  const [skills, setSkills] = useState<LearnedSkill[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // 用于去重的已知事件 ID 集合
  const knownEventIdsRef = useRef<Set<string>>(new Set());

  // SSE 实时事件流
  const { events: sseEvents, connected: sseConnected } = useSSE(groupId, baseUrl);

  /**
   * 拉取初始快照和 Skill 列表
   */
  const loadInitialData = useCallback(async () => {
    if (!groupId) {
      setLoading(false);
      return;
    }

    setLoading(true);
    setError(null);
    knownEventIdsRef.current = new Set();

    try {
      const [snap, skillList] = await Promise.all([
        fetchDashboard(groupId, baseUrl || undefined),
        fetchSkills(groupId, baseUrl || undefined),
      ]);

      // 记录初始快照中的事件 ID（用于后续 SSE 去重）
      snap.recent_timeline.forEach((item) => {
        knownEventIdsRef.current.add(item.id);
      });

      setSnapshot(snap);
      setSkills(skillList);
    } catch (err) {
      const message = err instanceof Error ? err.message : '数据加载失败';
      setError(message);
    } finally {
      setLoading(false);
    }
  }, [groupId, baseUrl]);

  // 初始化时加载数据
  useEffect(() => {
    void loadInitialData();
  }, [loadInitialData]);

  /**
   * 合并 SSE 新事件到 snapshot.timeline
   *
   * 业务逻辑说明：
   * 过滤掉已存在于快照中的事件（按 id 去重），
   * 将新事件追加到 timeline 尾部
   */
  useEffect(() => {
    if (sseEvents.length === 0) return;

    // 找出尚未在 timeline 中的新事件
    const newItems: LedgerTimelineItem[] = [];
    for (const event of sseEvents) {
      if (!knownEventIdsRef.current.has(event.id)) {
        knownEventIdsRef.current.add(event.id);
        newItems.push(event);
      }
    }

    if (newItems.length === 0) return;

    // 追加新事件并更新计数
    // 使用函数式更新（prev =>），不依赖外部 snapshot 变量
    // 限制 timeline 最多保留 500 条，防止长时间运行导致内存溢出
    const MAX_TIMELINE_ITEMS = 500;
    setSnapshot((prev) => {
      if (!prev) return prev;
      const merged = [...prev.recent_timeline, ...newItems];
      return {
        ...prev,
        recent_timeline: merged.length > MAX_TIMELINE_ITEMS ? merged.slice(-MAX_TIMELINE_ITEMS) : merged,
        total_events: prev.total_events + newItems.length,
      };
    });
    // 依赖项说明：移除 snapshot 依赖，避免 setSnapshot 触发的重渲染导致 effect 重复运行
    // setSnapshot 内部使用函数式更新（prev => ...），不需要外部 snapshot 值
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sseEvents]);

  /**
   * 更新 Agent 状态（通过 SSE 事件中的 agent 相关 kind 触发）
   *
   * 业务逻辑说明：
   * 当收到 actor_joined/actor_left/heartbeat 类事件时，刷新 Agent 列表
   */
  useEffect(() => {
    if (sseEvents.length === 0 || !groupId) return;

    const lastEvent = sseEvents[sseEvents.length - 1];
    const agentRelatedKinds = ['actor_joined', 'actor_left', 'heartbeat', 'ActorJoined', 'ActorLeft', 'Heartbeat'];

    if (agentRelatedKinds.includes(lastEvent.kind)) {
      // 重新拉取 Agent 状态（轻量级操作）
      fetchDashboard(groupId, baseUrl || undefined)
        .then((newSnap) => {
          setSnapshot((prev) => {
            if (!prev) return prev;
            return {
              ...prev,
              agents: newSnap.agents as AgentStatusView[],
            };
          });
        })
        .catch(() => {
          // Agent 状态刷新失败不影响主流程
        });
    }
  }, [sseEvents, groupId, baseUrl]);

  /**
   * 提升 Skill 到正式 Skill 库
   *
   * @param skillId - 要提升的 Skill ID
   */
  const handlePromoteSkill = useCallback(
    async (skillId: string) => {
      if (!groupId) return;
      try {
        await promoteSkill(groupId, skillId, baseUrl || undefined);
        // 提升成功后刷新 Skill 列表
        const updatedSkills = await fetchSkills(groupId, baseUrl || undefined);
        setSkills(updatedSkills);
      } catch (err) {
        const message = err instanceof Error ? err.message : 'Skill 提升失败';
        setError(message);
      }
    },
    [groupId, baseUrl],
  );

  /** 手动刷新快照 */
  const refresh = useCallback(() => {
    void loadInitialData();
  }, [loadInitialData]);

  return {
    snapshot,
    skills,
    loading,
    error,
    sseConnected,
    handlePromoteSkill,
    refresh,
  };
}
