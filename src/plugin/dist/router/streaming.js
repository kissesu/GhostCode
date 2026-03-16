function parseStreamEvent(line) {
  const trimmed = line.trim();
  if (trimmed.length === 0) {
    return null;
  }
  let parsed;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null || !("type" in parsed) || typeof parsed["type"] !== "string") {
    return null;
  }
  return parsed;
}
class StreamingHandler {
  /** 锁定的会话 ID，首次从事件中提取后不再更新 */
  _sessionId = null;
  /** 流是否已结束（complete 或 error 事件到达后置为 true） */
  _complete = false;
  /** 用户传入的回调集合 */
  _callbacks;
  constructor(callbacks) {
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
  handleLine(line) {
    const event = parseStreamEvent(line);
    if (event === null) {
      return;
    }
    if (this._sessionId === null && event.session_id !== void 0) {
      this._sessionId = event.session_id;
    }
    switch (event.type) {
      case "init":
        this._callbacks.onInit?.(event);
        break;
      case "progress":
        this._callbacks.onProgress?.(event);
        break;
      case "agent_message":
        this._callbacks.onAgentMessage?.(event);
        break;
      case "complete":
        this._complete = true;
        this._callbacks.onComplete?.(event);
        break;
      case "error":
        this._complete = true;
        this._callbacks.onError?.(event);
        break;
      default:
        break;
    }
  }
  /**
   * 获取当前锁定的会话 ID
   *
   * @returns 首次 session_id，若尚未收到则为 null
   */
  getSessionId() {
    return this._sessionId;
  }
  /**
   * 判断流是否已结束
   *
   * @returns true 表示收到 complete 或 error 事件，false 表示流仍在进行中
   */
  isComplete() {
    return this._complete;
  }
}
export {
  StreamingHandler,
  parseStreamEvent
};
//# sourceMappingURL=streaming.js.map