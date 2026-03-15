/*
 * @file stream.rs
 * @description JSON Stream 统一解析器
 *              将 Codex/Claude/Gemini 三种后端的 JSON 输出格式统一解析为 StreamEvent
 *              支持有状态 SESSION_ID 锁定机制，保证会话 ID 一旦确定不被覆盖
 * @author Atlas.oi
 * @date 2026-03-02
 */

// 参考: ccg-workflow/codeagent-wrapper/parser.go:72-163 - UnifiedEvent 结构和后端检测逻辑

use serde::Deserialize;
use serde_json::Value;

// ============================================================
// 公开类型定义
// ============================================================

/// 统一流事件类型
///
/// 对应三种后端的不同事件语义，映射到统一的五种类型：
/// - Codex: thread.started → Init, item.completed(agent_message) → AgentMessage
/// - Claude: result → AgentMessage
/// - Gemini: delta → Progress, status=success → Complete
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEventKind {
    /// 会话初始化（Codex thread.started / Gemini init）
    Init,
    /// 执行进度增量（Gemini delta 流式输出）
    Progress,
    /// Agent 核心输出内容（Codex agent_message / Claude result）
    AgentMessage,
    /// 执行完成（Gemini status=success / Codex thread.completed）
    Complete,
    /// 错误事件
    Error,
}

/// 统一流事件
///
/// 包含从原始 JSON 中提取的标准化字段，原始数据保留在 raw 字段中
#[derive(Debug, Clone)]
pub struct StreamEvent {
    /// 事件类型
    pub kind: StreamEventKind,
    /// 事件的主要文本内容（如有）
    pub content: Option<String>,
    /// 关联的会话 ID（如有）
    pub session_id: Option<String>,
    /// 原始 JSON 值，用于调试和扩展访问
    pub raw: Value,
}

/// 流解析错误类型
#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    /// JSON 解析失败（内部使用，通常转为 Ok(None) 跳过）
    #[error("解析错误: {0}")]
    ParseError(String),
}

// ============================================================
// 内部用于 JSON 解析的统一事件结构
// 参考: ccg-workflow/codeagent-wrapper/parser.go:72-90 - UnifiedEvent
// ============================================================

/// 统一 JSON 事件结构，一次反序列化覆盖所有后端字段
/// 参考: ccg-workflow/codeagent-wrapper/parser.go:72-90
#[derive(Debug, Deserialize)]
struct RawEvent {
    // 通用字段
    #[serde(rename = "type")]
    event_type: Option<String>,

    // Codex 特有字段
    thread_id: Option<String>,
    item: Option<Value>,

    // Claude 特有字段
    subtype: Option<String>,
    session_id: Option<String>,
    result: Option<String>,

    // Gemini 特有字段
    role: Option<String>,
    content: Option<String>,
    delta: Option<bool>,
    status: Option<String>,
}

// ============================================================
// 后端类型枚举（内部使用）
// ============================================================

/// 后端检测结果
enum Backend {
    /// OpenAI Codex（codex CLI）
    Codex,
    /// Anthropic Claude（claude CLI）
    Claude,
    /// Google Gemini（gemini CLI）
    Gemini,
    /// 无法识别的后端，跳过该事件
    Unknown,
}

// ============================================================
// StreamParser 实现
// ============================================================

/// JSON 流解析器（有状态）
///
/// 维护会话 SESSION_ID 的锁定状态：
/// - 第一次从任意后端事件中提取到 session_id 后锁定
/// - 后续事件中发现不同的 session_id 时忽略，保持锁定值不变
pub struct StreamParser {
    /// 已锁定的会话 ID，一旦设置不再覆盖
    locked_session_id: Option<String>,
}

impl Default for StreamParser {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamParser {
    /// 创建新的流解析器实例
    pub fn new() -> Self {
        Self {
            locked_session_id: None,
        }
    }

    /// 解析一行 JSON 文本，返回统一的 StreamEvent
    ///
    /// 业务逻辑：
    /// 1. 尝试将文本反序列化为 RawEvent
    /// 2. 检测后端类型（Codex/Claude/Gemini）
    /// 3. 根据后端类型提取事件语义和 session_id
    /// 4. 更新 SESSION_ID 锁定状态
    ///
    /// @param line - 一行 JSON 文本（来自后端 CLI 的 stdout）
    /// @returns Ok(Some(event)) - 成功解析的事件
    /// @returns Ok(None) - 无法识别或格式错误，跳过此行
    /// @returns Err(_) - 内部错误（当前实现不会返回）
    pub fn parse_line(&mut self, line: &str) -> Result<Option<StreamEvent>, StreamError> {
        // ============================================
        // 第一步：反序列化原始 JSON
        // 格式错误或非 JSON 内容直接跳过（返回 None）
        // ============================================
        let raw_value: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };

        let raw_event: RawEvent = match serde_json::from_value(raw_value.clone()) {
            Ok(e) => e,
            Err(_) => return Ok(None),
        };

        // ============================================
        // 第二步：检测后端类型
        // 参考: ccg-workflow/codeagent-wrapper/parser.go:165-180
        // 通过字段存在性判断来源后端，优先级：Codex > Claude > Gemini
        // ============================================
        let backend = detect_backend(&raw_event);

        // ============================================
        // 第三步：根据后端类型解析事件语义
        // 提取 kind、content、session_id 并更新锁定状态
        // ============================================
        let result = match backend {
            Backend::Codex => self.parse_codex_event(&raw_event, raw_value),
            Backend::Claude => self.parse_claude_event(&raw_event, raw_value),
            Backend::Gemini => self.parse_gemini_event(&raw_event, raw_value),
            Backend::Unknown => None,
        };

        Ok(result)
    }

    /// 获取已锁定的 SESSION_ID
    ///
    /// 返回 None 表示尚未接收到任何能提取 session_id 的事件
    pub fn session_id(&self) -> Option<&str> {
        self.locked_session_id.as_deref()
    }

    // ============================================================
    // 内部：SESSION_ID 锁定机制
    // ============================================================

    /// 尝试锁定 session_id
    ///
    /// 规则：只有在尚未锁定时才接受新值，锁定后忽略所有后续值
    fn try_lock_session_id(&mut self, session_id: &str) {
        if self.locked_session_id.is_none() {
            self.locked_session_id = Some(session_id.to_string());
        }
    }

    // ============================================================
    // 内部：各后端事件解析
    // ============================================================

    /// 解析 Codex 后端事件
    ///
    /// Codex 事件类型映射：
    /// - thread.started → Init（锁定 thread_id 为 session_id）
    /// - thread.completed / turn.completed → Complete
    /// - item.completed(agent_message) → AgentMessage
    /// - 其他 item.completed → Progress
    fn parse_codex_event(&mut self, event: &RawEvent, raw: Value) -> Option<StreamEvent> {
        let event_type = event.event_type.as_deref().unwrap_or("");

        match event_type {
            "thread.started" => {
                // 锁定 thread_id 作为 session_id
                if let Some(ref tid) = event.thread_id {
                    self.try_lock_session_id(tid);
                }
                Some(StreamEvent {
                    kind: StreamEventKind::Init,
                    content: None,
                    session_id: event.thread_id.clone(),
                    raw,
                })
            }
            "thread.completed" | "turn.completed" => {
                // 完成事件，如有 thread_id 也尝试锁定
                if let Some(ref tid) = event.thread_id {
                    self.try_lock_session_id(tid);
                }
                Some(StreamEvent {
                    kind: StreamEventKind::Complete,
                    content: None,
                    session_id: event.thread_id.clone(),
                    raw,
                })
            }
            "item.completed" => {
                // 根据 item.type 决定事件类型
                let item_type = event
                    .item
                    .as_ref()
                    .and_then(|v| v.get("type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let (kind, content) = match item_type {
                    "agent_message" => {
                        // 提取 item.content 或 item.text 字段
                        let content = event
                            .item
                            .as_ref()
                            .and_then(|v| v.get("content").or_else(|| v.get("text")))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        (StreamEventKind::AgentMessage, content)
                    }
                    _ => (StreamEventKind::Progress, None),
                };

                Some(StreamEvent {
                    kind,
                    content,
                    session_id: self.locked_session_id.clone(),
                    raw,
                })
            }
            _ => None,
        }
    }

    /// 解析 Claude 后端事件
    ///
    /// Claude 事件类型映射：
    /// - type=result，result 字段非空 → AgentMessage（锁定 session_id）
    /// - 其他 → None（跳过）
    fn parse_claude_event(&mut self, event: &RawEvent, raw: Value) -> Option<StreamEvent> {
        // Claude result 事件：提取 result 字段内容
        if let Some(ref result_text) = event.result {
            // 锁定 session_id（如有）
            if let Some(ref sid) = event.session_id {
                self.try_lock_session_id(sid);
            }
            return Some(StreamEvent {
                kind: StreamEventKind::AgentMessage,
                content: Some(result_text.clone()),
                session_id: event.session_id.clone(),
                raw,
            });
        }

        // subtype 存在时作为进度事件处理
        if event.subtype.is_some() {
            return Some(StreamEvent {
                kind: StreamEventKind::Progress,
                content: event.content.clone(),
                session_id: self.locked_session_id.clone(),
                raw,
            });
        }

        None
    }

    /// 解析 Gemini 后端事件
    ///
    /// Gemini stream-json 事件类型映射：
    /// - role=assistant → AgentMessage（无论是否 delta，assistant 响应都是最终结果）
    /// - role=user → 跳过（用户输入回显，不需要收集）
    /// - type=init → Init（锁定 session_id）
    /// - status=success → Complete
    /// - delta=true（非 role 事件）→ Progress
    ///
    /// 重要：role 检查必须在 delta 检查之前！
    /// Gemini CLI v0.33.1 的 assistant 消息带有 delta:true 标记，
    /// 如果先检查 delta 会把 assistant 响应错误归类为 Progress，
    /// 同时 user 消息（无 delta）会被末尾的 role 检查捕获为 AgentMessage，
    /// 导致 wrapper 输出用户 prompt 而非 AI 响应。
    fn parse_gemini_event(&mut self, event: &RawEvent, raw: Value) -> Option<StreamEvent> {
        // ============================================
        // 第一优先级：role=assistant → AgentMessage
        // assistant 的流式响应带 delta:true，但仍然是最终结果
        // 必须在 delta 检查之前命中，否则会被错误归类为 Progress
        // ============================================
        if event.role.as_deref() == Some("assistant") {
            return Some(StreamEvent {
                kind: StreamEventKind::AgentMessage,
                content: event.content.clone(),
                session_id: self.locked_session_id.clone(),
                raw,
            });
        }

        // ============================================
        // 第二优先级：role=user → 跳过
        // Gemini stream-json 会回显用户输入，这不是 AI 响应
        // 之前此消息被末尾的 role.is_some() 捕获为 AgentMessage，导致返回 prompt
        // ============================================
        if event.role.as_deref() == Some("user") {
            return None;
        }

        // type=init → Init（锁定 session_id）
        if event.event_type.as_deref() == Some("init") {
            if let Some(ref sid) = event.session_id {
                self.try_lock_session_id(sid);
            }
            return Some(StreamEvent {
                kind: StreamEventKind::Init,
                content: None,
                session_id: event.session_id.clone(),
                raw,
            });
        }

        // status=success 完成事件 → Complete
        if event.status.as_deref() == Some("success") {
            return Some(StreamEvent {
                kind: StreamEventKind::Complete,
                content: None,
                session_id: self.locked_session_id.clone(),
                raw,
            });
        }

        // status 存在但不是 success（如 error）
        if let Some(ref status) = event.status {
            if status != "success" {
                return Some(StreamEvent {
                    kind: StreamEventKind::Error,
                    content: Some(format!("状态异常: {}", status)),
                    session_id: self.locked_session_id.clone(),
                    raw,
                });
            }
        }

        // delta 增量流式输出（非 role 事件）→ Progress
        if event.delta == Some(true) {
            return Some(StreamEvent {
                kind: StreamEventKind::Progress,
                content: event.content.clone(),
                session_id: self.locked_session_id.clone(),
                raw,
            });
        }

        None
    }
}

// ============================================================
// 后端检测函数（独立函数便于测试和复用）
// 参考: ccg-workflow/codeagent-wrapper/parser.go:165-180
// ============================================================

/// 根据 JSON 字段存在性检测后端类型
///
/// 检测优先级：Codex > Claude > Gemini > Unknown
/// - Codex: 有 thread_id，或 type 为 turn.* / thread.*，或 item 字段存在且有 type 子字段
/// - Claude: 有 subtype，或 result 字段非空，或 (type=result 且 status 为空)
/// - Gemini: 有 role，或 delta，或 status，或 (type=init 且 session_id 存在)
fn detect_backend(event: &RawEvent) -> Backend {
    // ============================================
    // 检测 Codex：thread_id 或特定 type 前缀
    // 参考: ccg-workflow/codeagent-wrapper/parser.go:166-174
    // ============================================
    let is_codex = event.thread_id.is_some()
        || matches!(
            event.event_type.as_deref(),
            Some("turn.completed") | Some("turn.started") | Some("thread.started") | Some("thread.completed")
        )
        || event
            .item
            .as_ref()
            .and_then(|v| v.get("type"))
            .and_then(|v| v.as_str())
            .is_some();

    if is_codex {
        return Backend::Codex;
    }

    // ============================================
    // 检测 Claude：subtype 或 result 字段
    // 参考: ccg-workflow/codeagent-wrapper/parser.go:175-178
    // ============================================
    let is_claude = event.subtype.is_some()
        || event.result.is_some()
        || (event.event_type.as_deref() == Some("result")
            && event.session_id.is_some()
            && event.status.is_none());

    if is_claude {
        return Backend::Claude;
    }

    // ============================================
    // 检测 Gemini：role / delta / status 字段
    // 参考: ccg-workflow/codeagent-wrapper/parser.go:179-180
    // ============================================
    let is_gemini = event.role.is_some()
        || event.delta.is_some()
        || event.status.is_some()
        || (event.event_type.as_deref() == Some("init") && event.session_id.is_some());

    if is_gemini {
        return Backend::Gemini;
    }

    Backend::Unknown
}

// ============================================================
// 单元测试（内部逻辑验证）
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_codex_by_thread_id() {
        // 验证 detect_backend 能通过 thread_id 识别 Codex
        let event = RawEvent {
            event_type: None,
            thread_id: Some("th_abc".to_string()),
            item: None,
            subtype: None,
            session_id: None,
            result: None,
            role: None,
            content: None,
            delta: None,
            status: None,
        };
        assert!(matches!(detect_backend(&event), Backend::Codex));
    }

    #[test]
    fn detect_gemini_by_status() {
        // 验证 detect_backend 能通过 status 字段识别 Gemini
        let event = RawEvent {
            event_type: None,
            thread_id: None,
            item: None,
            subtype: None,
            session_id: None,
            result: None,
            role: None,
            content: None,
            delta: None,
            status: Some("success".to_string()),
        };
        assert!(matches!(detect_backend(&event), Backend::Gemini));
    }
}
