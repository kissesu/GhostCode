/**
 * @file SkillPanel.tsx
 * @description Skill 候选面板组件，展示已学习的 Skill 并支持提升操作
 *
 * 业务逻辑说明：
 * 1. 展示所有已学习的 Skill，显示名称、质量分、来源和标签
 * 2. 质量分 >= 70 的 Skill 显示"Promote"按钮
 * 3. 点击 Promote 调用回调，触发父组件的提升逻辑
 * 4. 按质量分降序排列
 *
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { useState } from 'react';
import type { LearnedSkill } from '../api/client';

/** SkillPanel 组件属性 */
interface SkillPanelProps {
  /** 已学习的 Skill 列表 */
  skills: LearnedSkill[];
  /** Promote 操作回调 */
  onPromote: (id: string) => Promise<void>;
}

/**
 * 根据质量分返回颜色
 *
 * @param quality - 质量分（0-100）
 * @returns CSS 颜色值
 */
function getQualityColor(quality: number): string {
  if (quality >= 80) return 'var(--accent-green)';
  if (quality >= 60) return 'var(--accent-yellow)';
  return 'var(--accent-red)';
}

/**
 * 获取来源类型的显示文本
 *
 * @param source - 来源类型
 * @returns 中文显示文本
 */
function getSourceLabel(source: LearnedSkill['source']): string {
  switch (source) {
    case 'extracted':
      return '自动提取';
    case 'promoted':
      return '已提升';
    case 'manual':
      return '手动创建';
    default:
      return source;
  }
}

/**
 * 单个 Skill 卡片
 */
function SkillCard({
  skill,
  onPromote,
}: {
  skill: LearnedSkill;
  onPromote: (id: string) => Promise<void>;
}) {
  // 提升操作的加载状态
  const [promoting, setPromoting] = useState(false);
  const qualityColor = getQualityColor(skill.quality);

  /**
   * 处理 Promote 点击
   */
  const handlePromote = async () => {
    setPromoting(true);
    try {
      await onPromote(skill.id);
    } finally {
      setPromoting(false);
    }
  };

  return (
    <div
      className="card p-3 flex flex-col gap-2"
      data-testid="skill-card"
    >
      {/* 头部：名称 + 质量分 */}
      <div className="flex items-start justify-between gap-2">
        <span
          className="text-sm font-medium leading-tight"
          style={{ color: 'var(--text-primary)' }}
        >
          {skill.name}
        </span>
        <span
          className="text-xs font-mono font-bold shrink-0 px-1.5 py-0.5 rounded"
          style={{
            color: qualityColor,
            backgroundColor: `${qualityColor}22`,
          }}
        >
          Q{skill.quality}
        </span>
      </div>

      {/* 描述 */}
      <p
        className="text-xs leading-relaxed line-clamp-2"
        style={{ color: 'var(--text-secondary)' }}
      >
        {skill.description}
      </p>

      {/* 标签列表 */}
      {skill.tags.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {skill.tags.slice(0, 4).map((tag) => (
            <span
              key={tag}
              className="text-xs px-1 py-0.5 rounded"
              style={{
                color: 'var(--text-muted)',
                backgroundColor: 'var(--border-subtle)',
              }}
            >
              {tag}
            </span>
          ))}
        </div>
      )}

      {/* 底部：来源 + Promote 按钮 */}
      <div className="flex items-center justify-between mt-1">
        <span className="text-xs" style={{ color: 'var(--text-muted)' }}>
          {getSourceLabel(skill.source)} · 使用 {skill.usage_count} 次
        </span>
        {skill.quality >= 70 && skill.source !== 'promoted' && (
          <button
            className="text-xs px-2 py-1 rounded transition-colors disabled:opacity-50"
            style={{
              color: 'var(--accent-blue)',
              backgroundColor: 'var(--accent-blue)22',
              border: '1px solid var(--accent-blue)44',
            }}
            onClick={handlePromote}
            disabled={promoting}
            data-testid="promote-button"
          >
            {promoting ? '处理中...' : 'Promote'}
          </button>
        )}
      </div>
    </div>
  );
}

/**
 * Skill 候选面板组件
 *
 * @param skills - 已学习的 Skill 列表
 * @param onPromote - 提升操作回调
 */
export function SkillPanel({ skills, onPromote }: SkillPanelProps) {
  if (skills.length === 0) {
    return (
      <div
        className="flex items-center justify-center h-32 text-sm"
        style={{ color: 'var(--text-muted)' }}
      >
        暂无 Skill
      </div>
    );
  }

  // 按质量分降序排列
  const sortedSkills = [...skills].sort((a, b) => b.quality - a.quality);

  const promotableCount = skills.filter(
    (s) => s.quality >= 70 && s.source !== 'promoted',
  ).length;

  return (
    <div className="flex flex-col gap-3" data-testid="skill-panel">
      {/* 统计摘要 */}
      <div
        className="text-xs px-1"
        style={{ color: 'var(--text-muted)' }}
      >
        {skills.length} 个 Skill
        {promotableCount > 0 && (
          <span style={{ color: 'var(--accent-yellow)' }}>
            {' '}· {promotableCount} 个可提升
          </span>
        )}
      </div>

      {/* Skill 列表 */}
      <div className="flex flex-col gap-2">
        {sortedSkills.map((skill) => (
          <SkillCard key={skill.id} skill={skill} onPromote={onPromote} />
        ))}
      </div>
    </div>
  );
}
