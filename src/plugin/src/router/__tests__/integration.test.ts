/**
 * @file 多模型路由端到端集成测试
 * @description 验证路由决策 -> 模板渲染 -> 流式处理的完整链路
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { describe, it, expect } from 'vitest';
import { resolveRoute } from '../strategy';
import { renderTemplate, buildTaskPrompt, SOVEREIGNTY_RULE } from '../templates';
import { parseStreamEvent, StreamingHandler } from '../streaming';
import type { BackendName } from '../types';
import type { StreamEvent } from '../streaming';

// ============================================
// 场景 1: 前端任务路由到 Gemini 完整链路
// 验证：CSS 任务 → gemini 路由 → 追加主权规则 → 流式处理
// ============================================
describe('端到端: 前端任务路由', () => {
  it('CSS 任务路由到 gemini + 追加主权规则 + 流式处理', () => {
    // 第一步：路由决策
    const decision = resolveRoute('修改 CSS 样式');
    expect(decision.backend).toBe('gemini');
    expect(decision.confidence).toBeGreaterThan(0);

    // 第二步：构建提示词，验证追加了主权规则
    const prompt = buildTaskPrompt('修改 CSS 样式', decision.backend, { WORKDIR: '/tmp' });
    expect(prompt).toContain(SOVEREIGNTY_RULE);

    // 第三步：流式处理器收到事件后触发回调
    const received: StreamEvent[] = [];
    const handler = new StreamingHandler({
      onInit: (e) => received.push(e),
      onComplete: (e) => received.push(e),
    });

    handler.handleLine('{"type":"init","session_id":"css-sess-1"}');
    handler.handleLine('{"type":"complete","session_id":"css-sess-1"}');

    // 验证会话 ID 锁定且流结束
    expect(handler.getSessionId()).toBe('css-sess-1');
    expect(handler.isComplete()).toBe(true);
    expect(received).toHaveLength(2);
    // noUncheckedIndexedAccess 开启时，使用非空断言明确告知编译器此处元素必然存在
    expect(received[0]!.type).toBe('init');
    expect(received[1]!.type).toBe('complete');
  });
});

// ============================================
// 场景 2: 后端任务路由到 Codex 完整链路
// 验证：API 任务 → codex 路由 → 追加主权规则
// ============================================
describe('端到端: 后端任务路由', () => {
  it('API 任务路由到 codex + 追加主权规则', () => {
    // 第一步：路由决策
    const decision = resolveRoute('实现 API 接口');
    expect(decision.backend).toBe('codex');
    expect(decision.confidence).toBeGreaterThan(0);

    // 第二步：构建提示词，验证追加了主权规则
    const prompt = buildTaskPrompt('实现 API 接口', decision.backend, { WORKDIR: '/project' });
    expect(prompt).toContain(SOVEREIGNTY_RULE);

    // 主权规则必须附加在提示词末尾
    expect(prompt.endsWith(SOVEREIGNTY_RULE)).toBe(true);
  });
});

// ============================================
// 场景 3: Claude 默认路由 — 无主权规则
// 验证：通用任务 → claude 路由 → 不追加主权规则
// ============================================
describe('端到端: Claude 默认路由', () => {
  it('通用任务路由到 claude + 无主权规则', () => {
    // 第一步：路由决策，无关键词命中应走默认 claude
    const decision = resolveRoute('帮我重构这段代码');
    expect(decision.backend).toBe('claude');
    expect(decision.confidence).toBe(0);

    // 第二步：构建提示词，验证不包含主权规则
    const prompt = buildTaskPrompt('帮我重构这段代码', decision.backend, { WORKDIR: '/tmp' });
    expect(prompt).not.toContain(SOVEREIGNTY_RULE);

    // claude 后端提示词只含原始任务文本（无追加）
    expect(prompt).toBe('帮我重构这段代码');
  });
});

// ============================================
// 场景 4: 强制前缀覆盖 + 模板渲染
// 验证：/codex 前缀 → codex 路由（置信度 1）→ 变量替换正确
// ============================================
describe('端到端: 强制前缀路由', () => {
  it('/codex 前缀强制路由到 codex，置信度 1', () => {
    // 第一步：强制前缀路由决策
    const decision = resolveRoute('/codex 优化查询算法');
    expect(decision.backend).toBe('codex');
    expect(decision.confidence).toBe(1);
    expect(decision.reason).toContain('/codex');

    // 第二步：模板渲染变量替换
    const template = '在 {{WORKDIR}} 目录下执行优化查询算法任务';
    const rendered = renderTemplate(template, { WORKDIR: '/opt/project' });
    expect(rendered).toBe('在 /opt/project 目录下执行优化查询算法任务');

    // 未匹配的占位符保留原样
    const renderWithMissing = renderTemplate('{{WORKDIR}}/{{MISSING}}', { WORKDIR: '/root' });
    expect(renderWithMissing).toBe('/root/{{MISSING}}');

    // 第三步：buildTaskPrompt 验证 codex 追加主权规则
    const prompt = buildTaskPrompt(template, 'codex', { WORKDIR: '/opt/project' });
    expect(prompt).toContain('在 /opt/project 目录下');
    expect(prompt).toContain(SOVEREIGNTY_RULE);
  });
});

// ============================================
// 场景 5: 流式事件全流程 Init → Progress → AgentMessage → Complete
// 验证：完整事件序列顺序、内容和状态正确
// ============================================
describe('端到端: 流式事件全流程', () => {
  it('模拟完整的流式事件序列', () => {
    const events: StreamEvent[] = [];
    const handler = new StreamingHandler({
      onInit: (e) => events.push(e),
      onProgress: (e) => events.push(e),
      onAgentMessage: (e) => events.push(e),
      onComplete: (e) => events.push(e),
    });

    // 模拟 Daemon 推送的事件流
    handler.handleLine('{"type":"init","session_id":"sess-123"}');
    handler.handleLine('{"type":"progress","progress":50}');
    handler.handleLine('{"type":"agent_message","content":"正在分析代码..."}');
    handler.handleLine('{"type":"agent_message","content":"发现 3 个优化点"}');
    handler.handleLine('{"type":"complete","session_id":"sess-123"}');

    // 验证会话 ID 首次锁定且流结束
    expect(handler.getSessionId()).toBe('sess-123');
    expect(handler.isComplete()).toBe(true);

    // 验证事件数量和顺序
    // noUncheckedIndexedAccess 开启时，使用非空断言明确告知编译器此处元素必然存在
    expect(events).toHaveLength(5);
    expect(events[0]!.type).toBe('init');
    expect(events[1]!.type).toBe('progress');
    expect(events[1]!.progress).toBe(50);
    expect(events[2]!.type).toBe('agent_message');
    expect(events[2]!.content).toBe('正在分析代码...');
    expect(events[3]!.type).toBe('agent_message');
    expect(events[3]!.content).toBe('发现 3 个优化点');
    expect(events[4]!.type).toBe('complete');
  });
});

// ============================================
// 场景 6: 错误流 — 流式事件中的错误处理
// 验证：error 事件 → onError 回调触发 → isComplete=true
// ============================================
describe('端到端: 错误流处理', () => {
  it('Error 事件触发回调并标记流结束', () => {
    const errorEvents: StreamEvent[] = [];
    const completeEvents: StreamEvent[] = [];
    const handler = new StreamingHandler({
      onError: (e) => errorEvents.push(e),
      onComplete: (e) => completeEvents.push(e),
    });

    // 流开始后发生错误
    handler.handleLine('{"type":"init","session_id":"err-sess-1"}');
    handler.handleLine('{"type":"error","error":"后端连接超时"}');

    // 验证 error 触发了 onError 而非 onComplete
    // noUncheckedIndexedAccess 开启时，使用非空断言明确告知编译器此处元素必然存在
    expect(errorEvents).toHaveLength(1);
    expect(errorEvents[0]!.type).toBe('error');
    expect(errorEvents[0]!.error).toBe('后端连接超时');
    expect(completeEvents).toHaveLength(0);

    // error 事件也应标记流结束
    expect(handler.isComplete()).toBe(true);

    // 会话 ID 已在 init 阶段锁定
    expect(handler.getSessionId()).toBe('err-sess-1');

    // 流结束后再推送事件，回调不应重复触发（但 handleLine 不报错）
    handler.handleLine('{"type":"complete","session_id":"err-sess-1"}');
    // isComplete 已为 true，状态不变
    expect(handler.isComplete()).toBe(true);
  });
});

// ============================================
// 场景 7: 路由决策与模板的组合矩阵
// 验证：三种后端的主权规则差异 — claude 无规则，其余有规则
// ============================================
describe('端到端: 路由-模板组合矩阵', () => {
  it('三种后端的主权规则差异', () => {
    const backends: BackendName[] = ['codex', 'claude', 'gemini'];
    const task = '执行任务';
    const vars = { WORKDIR: '/workspace' };

    for (const backend of backends) {
      const prompt = buildTaskPrompt(task, backend, vars);

      if (backend === 'claude') {
        // Claude 是代码主权持有者，不追加主权规则
        expect(prompt).not.toContain(SOVEREIGNTY_RULE);
        expect(prompt).toBe(task);
      } else {
        // codex / gemini 必须追加主权规则，限制写入权限
        expect(prompt).toContain(SOVEREIGNTY_RULE);
        expect(prompt).toContain(task);
      }
    }
  });

  it('parseStreamEvent 解析各类有效事件', () => {
    // 验证 parseStreamEvent 能正确解析所有合法事件类型
    // 使用非空断言（!）代替可选链（?.），避免 exactOptionalPropertyTypes 引发的类型错误
    const initEvent = parseStreamEvent('{"type":"init","session_id":"s1"}');
    expect(initEvent).not.toBeNull();
    expect(initEvent!.type).toBe('init');
    expect(initEvent!.session_id).toBe('s1');

    const progressEvent = parseStreamEvent('{"type":"progress","progress":75}');
    expect(progressEvent!.type).toBe('progress');
    expect(progressEvent!.progress).toBe(75);

    const msgEvent = parseStreamEvent('{"type":"agent_message","content":"hello"}');
    expect(msgEvent!.type).toBe('agent_message');
    expect(msgEvent!.content).toBe('hello');

    const completeEvent = parseStreamEvent('{"type":"complete"}');
    expect(completeEvent!.type).toBe('complete');

    const errorEvent = parseStreamEvent('{"type":"error","error":"timeout"}');
    expect(errorEvent!.type).toBe('error');
    expect(errorEvent!.error).toBe('timeout');

    // 无效行返回 null
    expect(parseStreamEvent('')).toBeNull();
    expect(parseStreamEvent('not json')).toBeNull();
    expect(parseStreamEvent('{"no_type":true}')).toBeNull();
  });
});
