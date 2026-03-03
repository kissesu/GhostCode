/**
 * @file 流式输出处理器测试
 * @description StreamingHandler 和 parseStreamEvent 的单元测试。
 *              覆盖：事件解析（Init / AgentMessage / Complete / Error / 无效JSON）、
 *              session_id 首次锁定、回调分发、isComplete 状态机。
 * @author Atlas.oi
 * @date 2026-03-02
 */

import { describe, it, expect, vi } from 'vitest';
import { parseStreamEvent, StreamingHandler } from '../streaming';

// ============================================
// parseStreamEvent 单元测试
// ============================================

describe('parseStreamEvent', () => {
  it('解析 Init 事件', () => {
    const line = JSON.stringify({ type: 'init', session_id: 'sess-001' });
    const event = parseStreamEvent(line);
    expect(event).not.toBeNull();
    expect(event!.type).toBe('init');
    expect(event!.session_id).toBe('sess-001');
  });

  it('解析 AgentMessage 事件', () => {
    const line = JSON.stringify({
      type: 'agent_message',
      session_id: 'sess-001',
      content: '本小姐在处理任务',
    });
    const event = parseStreamEvent(line);
    expect(event).not.toBeNull();
    expect(event!.type).toBe('agent_message');
    expect(event!.content).toBe('本小姐在处理任务');
  });

  it('解析 Complete 事件', () => {
    const line = JSON.stringify({ type: 'complete', session_id: 'sess-001' });
    const event = parseStreamEvent(line);
    expect(event).not.toBeNull();
    expect(event!.type).toBe('complete');
  });

  it('解析 Error 事件', () => {
    const line = JSON.stringify({
      type: 'error',
      error: '任务执行失败',
    });
    const event = parseStreamEvent(line);
    expect(event).not.toBeNull();
    expect(event!.type).toBe('error');
    expect(event!.error).toBe('任务执行失败');
  });

  it('跳过无效 JSON', () => {
    const event = parseStreamEvent('这不是合法JSON{{{');
    expect(event).toBeNull();
  });
});

// ============================================
// StreamingHandler 单元测试
// ============================================

describe('StreamingHandler', () => {
  it('从首个事件提取 session_id', () => {
    const handler = new StreamingHandler({});
    const line = JSON.stringify({ type: 'init', session_id: 'sess-abc' });
    handler.handleLine(line);
    expect(handler.getSessionId()).toBe('sess-abc');
  });

  it('session_id 一旦锁定后续不被覆盖', () => {
    const handler = new StreamingHandler({});
    handler.handleLine(JSON.stringify({ type: 'init', session_id: 'first-id' }));
    handler.handleLine(
      JSON.stringify({ type: 'agent_message', session_id: 'second-id', content: 'hello' })
    );
    // 首次设置的 session_id 不应被覆盖
    expect(handler.getSessionId()).toBe('first-id');
  });

  it('触发 onAgentMessage 回调', () => {
    const onAgentMessage = vi.fn();
    const handler = new StreamingHandler({ onAgentMessage });
    const line = JSON.stringify({
      type: 'agent_message',
      session_id: 'sess-001',
      content: '处理中...',
    });
    handler.handleLine(line);
    expect(onAgentMessage).toHaveBeenCalledOnce();
    // mock.calls[0] 和 mock.calls[0][0] 已由 toHaveBeenCalledOnce 保证非空
    const firstCall = onAgentMessage.mock.calls[0];
    expect(firstCall).toBeDefined();
    expect(firstCall![0].content).toBe('处理中...');
  });

  it('触发 onComplete 回调', () => {
    const onComplete = vi.fn();
    const handler = new StreamingHandler({ onComplete });
    const line = JSON.stringify({ type: 'complete', session_id: 'sess-001' });
    handler.handleLine(line);
    expect(onComplete).toHaveBeenCalledOnce();
    expect(handler.isComplete()).toBe(true);
  });

  it('触发 onError 回调', () => {
    const onError = vi.fn();
    const handler = new StreamingHandler({ onError });
    const line = JSON.stringify({ type: 'error', error: '执行失败' });
    handler.handleLine(line);
    expect(onError).toHaveBeenCalledOnce();
    expect(handler.isComplete()).toBe(true);
  });
});
