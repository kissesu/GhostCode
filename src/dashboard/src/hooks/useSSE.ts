/**
 * @file useSSE.ts
 * @description SSE（Server-Sent Events）订阅 Hook，监听 ghostcode-web 实时事件流
 *
 * 业务逻辑说明：
 * 1. 建立 EventSource 连接到 /api/groups/:id/stream
 * 2. 接收 SSE data 并解析为 LedgerTimelineItem
 * 3. 连接断开时自动重连（最多 5 次，指数退避）
 * 4. 组件卸载时清理连接
 *
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import type { LedgerTimelineItem } from '../api/client';

/**
 * 将 SSE 推送的原始 Event JSON 转换为 LedgerTimelineItem 格式
 *
 * 业务逻辑说明：
 * SSE 推送的是账本原始 Event 对象（含 data: JSON 对象），
 * 而前端 LedgerTimelineItem 期望 data_summary: string（完整的 JSON 字符串）。
 * 此函数对齐两者格式，确保 SSE 实时事件与 REST 快照事件在 Timeline 中显示一致。
 *
 * @param raw - SSE 解析后的原始 JSON 对象（Event 结构）
 * @returns LedgerTimelineItem 格式对象
 */
/**
 * 类型守卫：校验 SSE 原始数据是否包含 LedgerTimelineItem 必需字段
 *
 * W4 修复：替代 as string 类型断言，在运行时验证字段存在性和类型，
 * 防止后端 Event 结构变化时前端静默产生错误数据。
 *
 * @param raw - SSE 解析后的原始 JSON 对象
 * @returns true 表示包含全部必需字段且类型正确
 */
function isValidRawEvent(raw: Record<string, unknown>): raw is {
  id: string; ts: string; kind: string; group_id: string; by: string; data?: unknown;
} {
  return typeof raw.id === 'string'
    && typeof raw.ts === 'string'
    && typeof raw.kind === 'string'
    && typeof raw.group_id === 'string'
    && typeof raw.by === 'string';
}

function rawEventToTimelineItem(raw: Record<string, unknown>): LedgerTimelineItem | null {
  // W4 修复：使用类型守卫替代 as string 断言，结构不匹配时返回 null 跳过
  if (!isValidRawEvent(raw)) {
    console.warn('SSE 事件字段缺失或类型错误，跳过:', raw);
    return null;
  }

  // data 字段：原始 Event 中为 JSON 对象，序列化为字符串，完整返回不截断
  let dataSummary = '';
  if (raw.data !== undefined && raw.data !== null) {
    dataSummary = typeof raw.data === 'string' ? raw.data : JSON.stringify(raw.data);
  }

  return {
    id: raw.id,
    ts: raw.ts,
    kind: raw.kind,
    group_id: raw.group_id,
    by: raw.by,
    data_summary: dataSummary,
  };
}

/** SSE Hook 返回值 */
export interface UseSSEResult {
  /** 已接收的事件列表（按时间顺序追加） */
  events: LedgerTimelineItem[];
  /** 当前是否已连接 */
  connected: boolean;
  /** 连接错误信息（null 表示无错误） */
  error: string | null;
  /** 手动清空事件列表 */
  clearEvents: () => void;
}

/** 自动重连配置 */
const RECONNECT_CONFIG = {
  // 最大重连次数
  maxAttempts: 5,
  // 初始重连延迟（毫秒）
  baseDelay: 1000,
  // 指数退避倍数
  backoffFactor: 2,
};

/**
 * SSE 订阅 Hook
 *
 * 业务逻辑说明：
 * 1. 根据 groupId 动态构建 SSE URL
 * 2. 每次 groupId 变化时重建连接
 * 3. 通过 useRef 持有 EventSource 实例，避免重复创建
 *
 * @param groupId - 要订阅的 Group ID（null 则不建立连接）
 * @param baseUrl - 后端基础 URL（可选）
 * @returns SSE Hook 状态和控制函数
 */
export function useSSE(groupId: string | null, baseUrl = ''): UseSSEResult {
  const [events, setEvents] = useState<LedgerTimelineItem[]>([]);
  const [connected, setConnected] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 持有 EventSource 实例引用，避免在 effect 闭包中过期
  const esRef = useRef<EventSource | null>(null);
  // 重连计数器
  const reconnectCountRef = useRef(0);
  // 重连定时器 ID
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  /**
   * 关闭当前 EventSource 并清理定时器
   */
  const closeConnection = useCallback(() => {
    if (reconnectTimerRef.current !== null) {
      clearTimeout(reconnectTimerRef.current);
      reconnectTimerRef.current = null;
    }
    if (esRef.current) {
      esRef.current.close();
      esRef.current = null;
    }
  }, []);

  /**
   * 建立 SSE 连接
   *
   * 业务逻辑说明：
   * 1. 构建 SSE URL 并创建 EventSource
   * 2. 注册 onopen、onmessage、onerror 处理器
   * 3. 错误时触发重连逻辑
   */
  const connect = useCallback(() => {
    if (!groupId) return;

    const url = `${baseUrl}/api/groups/${groupId}/stream`;
    const es = new EventSource(url);
    esRef.current = es;

    es.onopen = () => {
      setConnected(true);
      setError(null);
      // 连接成功后重置重连计数
      reconnectCountRef.current = 0;
    };

    es.onmessage = (event: MessageEvent) => {
      try {
        const parsed = JSON.parse(event.data as string) as Record<string, unknown>;
        // 心跳事件（后端连接建立时立即发送）只用于冲刷代理缓冲区，
        // 不加入 timeline 事件列表
        if (parsed.type === 'connected') {
          return;
        }
        // 将原始 Event 转换为 LedgerTimelineItem 格式
        // SSE 推送的是 Event（含 data 对象），前端期望 LedgerTimelineItem（含 data_summary 字符串）
        const item = rawEventToTimelineItem(parsed);
        if (item) {
          setEvents((prev) => [...prev, item]);
        }
      } catch {
        // JSON 解析失败时记录警告但不中断流
        console.warn('SSE 消息解析失败:', event.data);
      }
    };

    es.onerror = () => {
      setConnected(false);
      es.close();
      esRef.current = null;

      // 达到最大重连次数则停止
      if (reconnectCountRef.current >= RECONNECT_CONFIG.maxAttempts) {
        setError(`SSE 连接失败，已重试 ${RECONNECT_CONFIG.maxAttempts} 次`);
        return;
      }

      // 指数退避重连
      const delay =
        RECONNECT_CONFIG.baseDelay *
        Math.pow(RECONNECT_CONFIG.backoffFactor, reconnectCountRef.current);
      reconnectCountRef.current += 1;

      reconnectTimerRef.current = setTimeout(() => {
        connect();
      }, delay);
    };
  }, [groupId, baseUrl]);

  // groupId 变化时重建连接
  useEffect(() => {
    if (!groupId) {
      closeConnection();
      setConnected(false);
      setEvents([]);
      return;
    }

    // 重置状态并建立新连接
    reconnectCountRef.current = 0;
    setError(null);
    connect();

    // 组件卸载时关闭连接
    return closeConnection;
  }, [groupId, connect, closeConnection]);

  /** 清空已接收的事件列表 */
  const clearEvents = useCallback(() => {
    setEvents([]);
  }, []);

  return { events, connected, error, clearEvents };
}
