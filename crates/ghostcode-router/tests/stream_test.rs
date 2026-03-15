/**
 * @file stream_test.rs
 * @description ghostcode-router JSON Stream 解析器测试套件
 *              测试三种后端（Codex/Claude/Gemini）的 JSON 输出格式统一解析
 *              以及 SESSION_ID 锁定机制和鲁棒性处理
 * @author Atlas.oi
 * @date 2026-03-02
 */

// 参考: ccg-workflow/codeagent-wrapper/parser.go:72-163 - 后端检测与统一事件解析

use ghostcode_router::stream::*;

// ============================================================
// Codex 事件解析测试
// ============================================================

#[test]
fn parse_codex_thread_started() {
    // Codex thread.started 事件应解析为 Init 类型，并锁定 thread_id 作为 session_id
    let line = r#"{"type":"thread.started","thread_id":"th_abc123"}"#;
    let mut parser = StreamParser::new();
    let event = parser.parse_line(line).unwrap().unwrap();
    assert!(matches!(event.kind, StreamEventKind::Init));
    assert_eq!(parser.session_id(), Some("th_abc123"));
}

#[test]
fn parse_codex_agent_message() {
    // Codex item.completed agent_message 类型应解析为 AgentMessage 事件
    let line = r#"{"type":"item.completed","item":{"type":"agent_message","content":"hello world"}}"#;
    let mut parser = StreamParser::new();
    let event = parser.parse_line(line).unwrap().unwrap();
    assert!(matches!(event.kind, StreamEventKind::AgentMessage));
}

// ============================================================
// Claude 事件解析测试
// ============================================================

#[test]
fn parse_claude_result() {
    // Claude result 事件应解析为 AgentMessage 类型，并提取 content 和 session_id
    let line = r#"{"type":"result","result":"generated code here","session_id":"sess_xyz"}"#;
    let mut parser = StreamParser::new();
    let event = parser.parse_line(line).unwrap().unwrap();
    assert!(matches!(event.kind, StreamEventKind::AgentMessage));
    assert_eq!(event.content.as_deref(), Some("generated code here"));
    assert_eq!(parser.session_id(), Some("sess_xyz"));
}

// ============================================================
// Gemini 事件解析测试
// ============================================================

#[test]
fn parse_gemini_assistant_delta() {
    // 修复后：role=assistant 的事件无论是否带 delta=true，都应归类为 AgentMessage
    // 这是 Gemini CLI v0.33.1 的实际行为：assistant 响应带 delta:true 标记，
    // 但仍然是最终 AI 响应内容，不是中间进度事件
    let line = r#"{"role":"assistant","content":"hello","delta":true}"#;
    let mut parser = StreamParser::new();
    let event = parser.parse_line(line).unwrap().unwrap();
    assert!(matches!(event.kind, StreamEventKind::AgentMessage));
    assert_eq!(event.content, Some("hello".to_string()));
}

#[test]
fn parse_gemini_pure_delta_progress() {
    // 纯 delta 事件（无 role）仍应解析为 Progress 类型
    let line = r#"{"content":"processing...","delta":true}"#;
    let mut parser = StreamParser::new();
    let event = parser.parse_line(line).unwrap().unwrap();
    assert!(matches!(event.kind, StreamEventKind::Progress));
}

#[test]
fn parse_gemini_user_role_skipped() {
    // role=user 事件应被跳过（返回 None），避免用户输入回显被误认为 AI 响应
    let line = r#"{"role":"user","content":"1+1等于几？"}"#;
    let mut parser = StreamParser::new();
    let result = parser.parse_line(line).unwrap();
    assert!(result.is_none());
}

#[test]
fn parse_gemini_complete() {
    // Gemini status=success 完成事件应解析为 Complete 类型
    let line = r#"{"status":"success"}"#;
    let mut parser = StreamParser::new();
    let event = parser.parse_line(line).unwrap().unwrap();
    assert!(matches!(event.kind, StreamEventKind::Complete));
}

// ============================================================
// SESSION_ID 锁定机制测试
// ============================================================

#[test]
fn session_id_locked_on_first_occurrence() {
    // SESSION_ID 一旦锁定，后续事件中的不同 session_id 不应覆盖已锁定的值
    let mut parser = StreamParser::new();
    // 第一个事件设置 session_id（Codex thread.started）
    parser
        .parse_line(r#"{"type":"thread.started","thread_id":"first_id"}"#)
        .unwrap();
    // 第二个事件有不同 session_id（Claude result），不应覆盖
    parser
        .parse_line(r#"{"type":"result","result":"x","session_id":"second_id"}"#)
        .unwrap();
    // 应保留第一个锁定的 session_id
    assert_eq!(parser.session_id(), Some("first_id"));
}

// ============================================================
// 鲁棒性测试
// ============================================================

#[test]
fn malformed_json_returns_none() {
    // 格式错误的 JSON 行不应 panic，返回 Ok(None) 表示跳过该行
    let mut parser = StreamParser::new();
    let result = parser.parse_line("not valid json");
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn empty_json_object_returns_none() {
    // 空 JSON 对象无法判断后端类型，应返回 Ok(None) 跳过
    let mut parser = StreamParser::new();
    let result = parser.parse_line("{}");
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

// ============================================================
// 属性测试（PBT）- 任意输入不 panic
// ============================================================

use proptest::prelude::*;

proptest! {
    #[test]
    fn random_bytes_no_panic(data in "\\PC{0,500}") {
        // 任意随机字符串输入不应导致 panic
        let mut parser = StreamParser::new();
        let _ = parser.parse_line(&data);
    }
}
