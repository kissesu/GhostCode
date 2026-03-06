/**
 * @file client.ts
 * @description REST API 客户端，封装 ghostcode-web 后端接口调用
 *
 * 业务逻辑说明：
 * 1. 统一 baseUrl 配置（默认 http://127.0.0.1:7070）
 * 2. 封装通用 fetch，统一错误处理
 * 3. 导出各端点的类型化请求函数
 *
 * @author Atlas.oi
 * @date 2026-03-03
 */

// ============================================
// 类型定义（与 ghostcode-types Rust 结构体对齐）
// ============================================

/** 账本时间线条目 */
export interface LedgerTimelineItem {
  /** 事件唯一标识 */
  id: string;
  /** ISO 8601 UTC 时间戳 */
  ts: string;
  /** 事件类型字符串 */
  kind: string;
  /** 所属 Group ID */
  group_id: string;
  /** 触发者 Actor ID */
  by: string;
  /** 事件负载摘要（最多 200 字符的 JSON 字符串） */
  data_summary: string;
}

/** Agent 状态视图 */
export interface AgentStatusView {
  /** Actor ID */
  actor_id: string;
  /** Runtime 类型 */
  runtime: string;
  /** 最后已知状态 */
  status: 'active' | 'stopped' | 'unknown';
  /** 最后活动时间戳 */
  last_seen: string | null;
  /** Agent 显示名称（从 SubagentStart 事件的 agent_type 生成，如 "Code Reviewer"） */
  display_name?: string;
  /** Agent 类型标识（原始值，如 "feature-dev:code-reviewer"） */
  agent_type?: string;
}

/** Dashboard 快照（聚合视图） */
export interface DashboardSnapshot {
  /** Group ID */
  group_id: string;
  /** 快照生成时间戳 */
  snapshot_ts: string;
  /** 事件总数 */
  total_events: number;
  /** 活跃 Agent 列表 */
  agents: AgentStatusView[];
  /** 最近 N 条时间线 */
  recent_timeline: LedgerTimelineItem[];
}

/** 时间线分页查询结果 */
export interface TimelinePage {
  /** 本页条目 */
  items: LedgerTimelineItem[];
  /** 下一页游标（null 表示已到末尾） */
  next_cursor: string | null;
  /** 总事件数 */
  total: number;
}

/** 已学习的 Skill（简化视图） */
export interface LearnedSkill {
  /** 唯一标识符 */
  id: string;
  /** 人类可读名称 */
  name: string;
  /** 功能描述 */
  description: string;
  /** 质量分（0-100） */
  quality: number;
  /** 来源类型 */
  source: 'extracted' | 'promoted' | 'manual';
  /** 作用域 */
  scope: 'user' | 'project';
  /** 使用次数 */
  usage_count: number;
  /** 标签列表 */
  tags: string[];
}

// ============================================
// API 客户端配置
// ============================================

/** 默认后端地址，开发时通过 Vite proxy 转发 */
const DEFAULT_BASE_URL = '';

/** API 请求错误类型 */
export class ApiError extends Error {
  constructor(
    message: string,
    public readonly status: number,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

/**
 * 通用 fetch 封装
 *
 * 业务逻辑说明：
 * 1. 拼接 baseUrl 和路径
 * 2. 检查 HTTP 状态码，非 2xx 抛出 ApiError
 * 3. 解析 JSON 响应体并返回
 *
 * @param path - API 路径（不含 baseUrl）
 * @param options - fetch 选项
 * @param baseUrl - 后端基础 URL
 * @returns 解析后的 JSON 数据
 * @throws {ApiError} 当 HTTP 状态码非 2xx 时抛出
 */
async function apiFetch<T>(
  path: string,
  options: RequestInit = {},
  baseUrl: string = DEFAULT_BASE_URL,
): Promise<T> {
  const url = `${baseUrl}${path}`;
  const response = await fetch(url, {
    headers: {
      'Content-Type': 'application/json',
      ...options.headers,
    },
    ...options,
  });

  if (!response.ok) {
    throw new ApiError(
      `API 请求失败: ${response.status} ${response.statusText}`,
      response.status,
    );
  }

  return response.json() as Promise<T>;
}

// ============================================
// 各端点请求函数
// ============================================

/** 活跃 Group 响应 */
export interface ActiveGroupResponse {
  /** 当前活跃的 Group ID（null 表示无活跃 Group） */
  group_id: string | null;
}

/**
 * 查询当前活跃的 Group
 *
 * 后端自动扫描 groups 目录，返回最近有账本活动的 group
 *
 * @param baseUrl - 后端基础 URL（可选）
 * @returns 活跃 Group 响应
 */
export async function fetchActiveGroup(
  baseUrl?: string,
): Promise<ActiveGroupResponse> {
  return apiFetch<ActiveGroupResponse>('/api/active-group', {}, baseUrl);
}

/**
 * 获取 Dashboard 快照
 *
 * @param groupId - Group ID
 * @param baseUrl - 后端基础 URL（可选）
 * @returns Dashboard 快照数据
 */
export async function fetchDashboard(
  groupId: string,
  baseUrl?: string,
): Promise<DashboardSnapshot> {
  return apiFetch<DashboardSnapshot>(
    `/api/groups/${groupId}/dashboard`,
    {},
    baseUrl,
  );
}

/**
 * 获取时间线分页数据
 *
 * @param groupId - Group ID
 * @param pageSize - 每页条数，默认 50
 * @param cursor - 分页游标（可选）
 * @param baseUrl - 后端基础 URL（可选）
 * @returns 时间线分页结果
 */
export async function fetchTimeline(
  groupId: string,
  pageSize = 50,
  cursor?: string,
  baseUrl?: string,
): Promise<TimelinePage> {
  const params = new URLSearchParams({ page_size: String(pageSize) });
  if (cursor) {
    params.set('cursor', cursor);
  }
  return apiFetch<TimelinePage>(
    `/api/groups/${groupId}/timeline?${params.toString()}`,
    {},
    baseUrl,
  );
}

/**
 * 获取 Agent 状态列表
 *
 * @param groupId - Group ID
 * @param baseUrl - 后端基础 URL（可选）
 * @returns Agent 状态视图数组
 */
export async function fetchAgents(
  groupId: string,
  baseUrl?: string,
): Promise<AgentStatusView[]> {
  return apiFetch<AgentStatusView[]>(
    `/api/groups/${groupId}/agents`,
    {},
    baseUrl,
  );
}

/**
 * 获取已学习的 Skill 列表
 *
 * @param groupId - Group ID
 * @param baseUrl - 后端基础 URL（可选）
 * @returns 已学习 Skill 数组
 */
export async function fetchSkills(
  groupId: string,
  baseUrl?: string,
): Promise<LearnedSkill[]> {
  return apiFetch<LearnedSkill[]>(
    `/api/groups/${groupId}/skills`,
    {},
    baseUrl,
  );
}

/**
 * 提升 Skill 到正式 Skill 库
 *
 * @param groupId - Group ID
 * @param skillId - 要提升的 Skill ID
 * @param baseUrl - 后端基础 URL（可选）
 * @returns 操作结果
 */
export async function promoteSkill(
  groupId: string,
  skillId: string,
  baseUrl?: string,
): Promise<{ ok: boolean }> {
  return apiFetch<{ ok: boolean }>(
    `/api/groups/${groupId}/skills/${skillId}/promote`,
    { method: 'POST' },
    baseUrl,
  );
}
