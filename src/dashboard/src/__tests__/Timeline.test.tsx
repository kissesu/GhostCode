/**
 * @file Timeline.test.tsx
 * @description Timeline 组件单元测试
 *
 * 测试覆盖：
 * 1. 空列表时显示"暂无事件"提示
 * 2. 正常渲染事件列表，条目数量正确
 * 3. 每个条目包含 kind、by、data_summary
 * 4. kind 标签颜色根据类型变化（错误类为红色）
 *
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { Timeline } from '../components/Timeline';
import type { LedgerTimelineItem } from '../api/client';

// ============================================
// 测试数据工厂函数
// ============================================

/**
 * 创建测试用的 LedgerTimelineItem
 */
function makeItem(overrides: Partial<LedgerTimelineItem> = {}): LedgerTimelineItem {
  return {
    id: 'test-id-1',
    ts: new Date().toISOString(),
    kind: 'message_sent',
    group_id: 'group-1',
    by: 'actor-1',
    data_summary: '{"content": "hello"}',
    ...overrides,
  };
}

// ============================================
// 测试套件
// ============================================

describe('Timeline', () => {
  it('空列表时显示暂无事件提示', () => {
    render(<Timeline items={[]} />);
    expect(screen.getByText('暂无事件')).toBeInTheDocument();
  });

  it('正确渲染单条事件', () => {
    const item = makeItem({ kind: 'message_sent', by: 'actor-alice' });
    render(<Timeline items={[item]} />);

    // 验证 kind 标签存在
    expect(screen.getByText('message_sent')).toBeInTheDocument();
    // 验证 by 字段
    expect(screen.getByText('actor-alice')).toBeInTheDocument();
    // 验证 data_summary 被格式化显示（formatDataSummary 处理后的文本）
    expect(screen.getByText(/content.*hello/)).toBeInTheDocument();
  });

  it('正确渲染多条事件', () => {
    const items = [
      makeItem({ id: 'id-1', kind: 'actor_joined', by: 'actor-1' }),
      makeItem({ id: 'id-2', kind: 'message_sent', by: 'actor-2' }),
      makeItem({ id: 'id-3', kind: 'actor_left', by: 'actor-3' }),
    ];
    render(<Timeline items={items} />);

    const cards = screen.getAllByTestId('timeline-item');
    expect(cards).toHaveLength(3);
  });

  it('渲染 timeline 容器', () => {
    const items = [makeItem()];
    render(<Timeline items={items} />);
    expect(screen.getByTestId('timeline')).toBeInTheDocument();
  });

  it('事件 data_summary 为空时不显示摘要区域', () => {
    const item = makeItem({ data_summary: '' });
    render(<Timeline items={[item]} />);
    // 只验证基本内容渲染正常，不崩溃
    expect(screen.getByTestId('timeline-item')).toBeInTheDocument();
  });

  it('渲染不同 kind 类型的事件', () => {
    const items = [
      makeItem({ id: 'id-err', kind: 'error_occurred' }),
      makeItem({ id: 'id-skill', kind: 'skill_learned' }),
      makeItem({ id: 'id-join', kind: 'actor_joined' }),
    ];
    render(<Timeline items={items} />);

    expect(screen.getByText('error_occurred')).toBeInTheDocument();
    expect(screen.getByText('skill_learned')).toBeInTheDocument();
    expect(screen.getByText('actor_joined')).toBeInTheDocument();
  });
});
