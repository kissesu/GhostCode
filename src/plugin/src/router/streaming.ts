/**
 * @file 流式输出处理器
 * @description 处理 Daemon 发送的多行 JSON 流式事件，提供 parseStreamEvent 函数
 *              和 StreamingHandler 类。协议对齐 Rust 侧 UnifiedEvent 枚举：
 *              Init / Progress / AgentMessage / Complete / Error。
 *              每个事件是一行 JSON，type 为 'complete' 或 'error' 表示流结束。
 * @author Atlas.oi
 * @date 2026-03-02
 */

// ============================================
// 流式事件类型定义
// 与 Rust 侧 UnifiedEvent 枚举对齐
// ============================================

/** 流式事件类型联合 */
export type StreamEventType =
  | 'init'
  | 'progress'
  | 'agent_message'
  | 'complete'
  | 'error';

/** 单个流式事件，对应 Rust 侧 UnifiedEvent 序列化后的 JSON 结构 */
export interface StreamEvent {
  /** 事件类型 */
  type: StreamEventType;
  /** 会话 ID，首次 init 事件中携带，后续可选 */
  session_id?: string;
  /** 消息内容（agent_message 事件携带） */
  content?: string;
  /** 进度百分比 0-100（progress 事件携带） */
  progress?: number;
  /** 错误信息（error 事件携带） */
  error?: string;
}

/** 流式事件回调接口 */
export interface StreamCallbacks {
  /** Init 事件回调：流开始时触发 */
  onInit?: (event: StreamEvent) => void;
  /** Progress 事件回调：进度更新时触发 */
  onProgress?: (event: StreamEvent) => void;
  /** AgentMessage 事件回调：有消息内容时触发 */
  onAgentMessage?: (event: StreamEvent) => void;
  /** Complete 事件回调：流正常结束时触发 */
  onComplete?: (event: StreamEvent) => void;
  /** Error 事件回调：流异常结束时触发 */
  onError?: (event: StreamEvent) => void;
}

// ============================================
// parseStreamEvent 函数
// ============================================

/**
 * 解析单行 JSON 为 StreamEvent
 *
 * 业务逻辑说明：
 * 1. 尝试 JSON.parse 解析
 * 2. 验证 type 字段存在（必填）
 * 3. 无效 JSON 或缺少 type 字段时返回 null（不 throw，容错处理）
 *
 * @param line - 单行 JSON 字符串（不含换行符）
 * @returns 解析成功返回 StreamEvent，解析失败返回 null
 */
export function parseStreamEvent(line: string): StreamEvent | null {
  // 空行直接跳过，避免 JSON.parse 无意义报错
  const trimmed = line.trim();
  if (trimmed.length === 0) {
    return null;
  }

  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    // 无效 JSON 静默返回 null，由调用方决定是否记录警告
    return null;
  }

  // 验证必填字段 type 存在且为字符串
  if (
    typeof parsed !== 'object' ||
    parsed === null ||
    !('type' in parsed) ||
    typeof (parsed as Record<string, unknown>)['type'] !== 'string'
  ) {
    return null;
  }

  return parsed as StreamEvent;
}

// ============================================
// StreamingHandler 类
// ============================================

/**
 * 流式响应处理器
 *
 * 维护流式会话的状态机：
 * - session_id 首次出现时锁定，后续事件不覆盖
 * - type 为 'complete' 或 'error' 时标记流结束
 * - 每行通过 handleLine() 传入，内部解析并分发到对应回调
 */
export class StreamingHandler {
  /** 锁定的会话 ID，首次从事件中提取后不再更新 */
  private _sessionId: string | null = null;

  /** 流是否已结束（complete 或 error 事件到达后置为 true） */
  private _complete: boolean = false;

  /** 用户传入的回调集合 */
  private readonly _callbacks: StreamCallbacks;

  constructor(callbacks: StreamCallbacks) {
    this._callbacks = callbacks;
  }

  /**
   * 处理一行流式数据
   *
   * 业务逻辑说明：
   * 1. 调用 parseStreamEvent 解析行内容
   * 2. 无效行（返回 null）静默跳过
   * 3. 首次出现 session_id 时锁定
   * 4. 根据 type 分发到对应回调
   * 5. complete 或 error 时标记流结束
   *
   * @param line - 单行原始字符串（可能包含前后空白）
   */
  handleLine(line: string): void {
    const event = parseStreamEvent(line);

    // 无效行静默跳过
    if (event === null) {
      return;
    }

    // ============================================
    // session_id 首次锁定
    // 一旦锁定，后续事件即使携带不同 session_id 也不覆盖
    // ============================================
    if (this._sessionId === null && event.session_id !== undefined) {
      this._sessionId = event.session_id;
    }

    // ============================================
    // 根据事件类型分发到对应回调
    // ============================================
    switch (event.type) {
      case 'init':
        this._callbacks.onInit?.(event);
        break;

      case 'progress':
        this._callbacks.onProgress?.(event);
        break;

      case 'agent_message':
        this._callbacks.onAgentMessage?.(event);
        break;

      case 'complete':
        // 标记流结束，触发 onComplete 回调
        this._complete = true;
        this._callbacks.onComplete?.(event);
        break;

      case 'error':
        // 错误也表示流结束，触发 onError 回调
        this._complete = true;
        this._callbacks.onError?.(event);
        break;

      default:
        // 未知事件类型静默忽略，向前兼容
        break;
    }
  }

  /**
   * 获取当前锁定的会话 ID
   *
   * @returns 首次 session_id，若尚未收到则为 null
   */
  getSessionId(): string | null {
    return this._sessionId;
  }

  /**
   * 判断流是否已结束
   *
   * @returns true 表示收到 complete 或 error 事件，false 表示流仍在进行中
   */
  isComplete(): boolean {
    return this._complete;
  }
}
