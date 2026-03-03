/**
 * @file useSSE.test.ts
 * @description useSSE Hook 单元测试
 *
 * 测试覆盖：
 * 1. groupId 为 null 时不建立连接，返回空状态
 * 2. groupId 有效时建立 EventSource 连接
 * 3. 接收 SSE 消息后正确解析并追加到 events 列表
 * 4. 连接建立后 connected 为 true
 * 5. clearEvents 调用后清空 events 列表
 *
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { LedgerTimelineItem } from '../api/client';
import { useSSE } from '../hooks/useSSE';

// ============================================
// Mock EventSource
// ============================================

/**
 * 模拟 EventSource 类，用于测试环境
 * jsdom 默认不支持 EventSource，需要手动 mock
 */
class MockEventSource {
  url: string;
  onopen: (() => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onerror: (() => void) | null = null;
  readyState = 0;

  // 全局实例注册，用于在测试中触发事件
  static instances: MockEventSource[] = [];

  constructor(url: string) {
    this.url = url;
    MockEventSource.instances.push(this);
  }

  close() {
    this.readyState = 2;
  }

  /** 测试辅助：触发 open 事件 */
  triggerOpen() {
    this.readyState = 1;
    this.onopen?.();
  }

  /** 测试辅助：触发 message 事件 */
  triggerMessage(data: string) {
    this.onmessage?.(new MessageEvent('message', { data }));
  }

  /** 测试辅助：触发 error 事件 */
  triggerError() {
    this.onerror?.();
  }
}

// ============================================
// 测试套件
// ============================================

describe('useSSE', () => {
  beforeEach(() => {
    MockEventSource.instances = [];
    // 替换全局 EventSource
    vi.stubGlobal('EventSource', MockEventSource);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.clearAllTimers();
  });

  it('groupId 为 null 时不建立连接', () => {
    const { result } = renderHook(() => useSSE(null));

    expect(result.current.connected).toBe(false);
    expect(result.current.events).toHaveLength(0);
    expect(result.current.error).toBeNull();
    expect(MockEventSource.instances).toHaveLength(0);
  });

  it('groupId 有效时建立 EventSource 连接', () => {
    renderHook(() => useSSE('group-1'));

    expect(MockEventSource.instances).toHaveLength(1);
    expect(MockEventSource.instances[0].url).toBe('/api/groups/group-1/stream');
  });

  it('触发 open 事件后 connected 变为 true', () => {
    const { result } = renderHook(() => useSSE('group-1'));

    act(() => {
      MockEventSource.instances[0].triggerOpen();
    });

    expect(result.current.connected).toBe(true);
  });

  it('接收 SSE 消息后正确追加到 events', () => {
    const { result } = renderHook(() => useSSE('group-1'));

    act(() => {
      MockEventSource.instances[0].triggerOpen();
    });

    const item: LedgerTimelineItem = {
      id: 'evt-1',
      ts: new Date().toISOString(),
      kind: 'message_sent',
      group_id: 'group-1',
      by: 'actor-1',
      data_summary: '{"content": "test"}',
    };

    act(() => {
      MockEventSource.instances[0].triggerMessage(JSON.stringify(item));
    });

    expect(result.current.events).toHaveLength(1);
    expect(result.current.events[0].id).toBe('evt-1');
    expect(result.current.events[0].kind).toBe('message_sent');
  });

  it('接收多条 SSE 消息后按顺序追加', () => {
    const { result } = renderHook(() => useSSE('group-1'));

    act(() => {
      MockEventSource.instances[0].triggerOpen();
    });

    const items: LedgerTimelineItem[] = [
      {
        id: 'evt-1',
        ts: new Date().toISOString(),
        kind: 'actor_joined',
        group_id: 'group-1',
        by: 'actor-1',
        data_summary: '',
      },
      {
        id: 'evt-2',
        ts: new Date().toISOString(),
        kind: 'message_sent',
        group_id: 'group-1',
        by: 'actor-2',
        data_summary: '',
      },
    ];

    act(() => {
      MockEventSource.instances[0].triggerMessage(JSON.stringify(items[0]));
      MockEventSource.instances[0].triggerMessage(JSON.stringify(items[1]));
    });

    expect(result.current.events).toHaveLength(2);
    expect(result.current.events[0].id).toBe('evt-1');
    expect(result.current.events[1].id).toBe('evt-2');
  });

  it('clearEvents 调用后清空 events 列表', () => {
    const { result } = renderHook(() => useSSE('group-1'));

    act(() => {
      MockEventSource.instances[0].triggerOpen();
    });

    const item: LedgerTimelineItem = {
      id: 'evt-1',
      ts: new Date().toISOString(),
      kind: 'test',
      group_id: 'group-1',
      by: 'actor-1',
      data_summary: '',
    };

    act(() => {
      MockEventSource.instances[0].triggerMessage(JSON.stringify(item));
    });

    expect(result.current.events).toHaveLength(1);

    act(() => {
      result.current.clearEvents();
    });

    expect(result.current.events).toHaveLength(0);
  });

  it('无效 JSON 消息不会导致崩溃', () => {
    const { result } = renderHook(() => useSSE('group-1'));

    act(() => {
      MockEventSource.instances[0].triggerOpen();
    });

    // 发送无效 JSON
    act(() => {
      MockEventSource.instances[0].triggerMessage('invalid-json-data');
    });

    // events 应仍为空，不崩溃
    expect(result.current.events).toHaveLength(0);
    expect(result.current.connected).toBe(true);
  });

  it('使用自定义 baseUrl 构建正确的 SSE URL', () => {
    renderHook(() => useSSE('group-2', 'http://127.0.0.1:7070'));

    expect(MockEventSource.instances[0].url).toBe(
      'http://127.0.0.1:7070/api/groups/group-2/stream',
    );
  });
});
