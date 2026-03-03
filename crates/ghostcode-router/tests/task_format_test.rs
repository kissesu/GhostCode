/**
 * @file task_format_test.rs
 * @description ghostcode-router task_format 模块的集成测试
 *              测试 ---TASK---/---CONTENT--- 分隔符格式的解析和序列化
 *              包含单任务、多任务、依赖、session_id、往返序列化等用例
 * @author Atlas.oi
 * @date 2026-03-02
 */

use ghostcode_router::task_format::*;
use proptest::prelude::*;

// ============================================
// 基本解析测试
// ============================================

#[test]
fn parse_single_task() {
    let input = "---TASK---\nid: task1\nbackend: codex\n---CONTENT---\n做一些事情\n";
    let tasks = parse_task_format(input).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, "task1");
    assert_eq!(tasks[0].backend, "codex");
    assert_eq!(tasks[0].task_text.trim(), "做一些事情");
}

#[test]
fn parse_multiple_tasks() {
    let input = "\
---TASK---
id: task1
backend: codex
---CONTENT---
第一个任务
---TASK---
id: task2
backend: gemini
---CONTENT---
第二个任务
";
    let tasks = parse_task_format(input).unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].id, "task1");
    assert_eq!(tasks[0].backend, "codex");
    assert_eq!(tasks[1].id, "task2");
    assert_eq!(tasks[1].backend, "gemini");
}

#[test]
fn parse_with_dependencies() {
    let input = "---TASK---\nid: task2\ndependencies: task1,task0\n---CONTENT---\n内容\n";
    let tasks = parse_task_format(input).unwrap();
    assert_eq!(tasks[0].dependencies, vec!["task1", "task0"]);
}

#[test]
fn session_id_sets_resume_mode() {
    let input = "---TASK---\nid: task1\nsession_id: sess123\n---CONTENT---\n内容\n";
    let tasks = parse_task_format(input).unwrap();
    assert_eq!(tasks[0].session_id, Some("sess123".to_string()));
    assert!(tasks[0].is_resume());
}

#[test]
fn missing_optional_fields_use_defaults() {
    let input = "---TASK---\nid: task1\n---CONTENT---\n内容\n";
    let tasks = parse_task_format(input).unwrap();
    // 默认 backend 为 codex
    assert_eq!(tasks[0].backend, "codex");
    assert!(tasks[0].dependencies.is_empty());
    assert!(tasks[0].session_id.is_none());
    assert!(tasks[0].workdir.is_none());
}

#[test]
fn multiline_content_preserved() {
    let input = "---TASK---\nid: task1\n---CONTENT---\n第一行\n第二行\n第三行\n";
    let tasks = parse_task_format(input).unwrap();
    assert!(tasks[0].task_text.contains("第一行"));
    assert!(tasks[0].task_text.contains("第二行"));
    assert!(tasks[0].task_text.contains("第三行"));
}

#[test]
fn empty_input_empty_result() {
    let tasks = parse_task_format("").unwrap();
    assert!(tasks.is_empty());
}

#[test]
fn workdir_parsed() {
    let input = "---TASK---\nid: task1\nworkdir: /tmp/myproject\n---CONTENT---\n内容\n";
    let tasks = parse_task_format(input).unwrap();
    assert_eq!(tasks[0].workdir, Some("/tmp/myproject".to_string()));
}

#[test]
fn roundtrip_serialize_parse() {
    let specs = vec![
        TaskSpec {
            id: "t1".into(),
            task_text: "任务内容\n多行".into(),
            workdir: Some("/tmp".into()),
            backend: "codex".into(),
            dependencies: vec!["t0".into()],
            session_id: None,
        },
        TaskSpec {
            id: "t2".into(),
            task_text: "第二个".into(),
            workdir: None,
            backend: "gemini".into(),
            dependencies: vec![],
            session_id: Some("sess1".into()),
        },
    ];
    let serialized = serialize_task_format(&specs);
    let parsed = parse_task_format(&serialized).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].id, "t1");
    assert_eq!(parsed[1].id, "t2");
    assert_eq!(parsed[0].backend, "codex");
    assert_eq!(parsed[1].session_id, Some("sess1".into()));
}

// ============================================
// proptest 往返属性测试
// 验证序列化后再解析的结果与原始数据一致
// ============================================

proptest! {
    #[test]
    fn proptest_roundtrip(
        id in "[a-z][a-z0-9_]{0,15}",
        task_text in "[^\x00]{1,100}",
        backend in prop_oneof![
            Just("codex".to_string()),
            Just("claude".to_string()),
            Just("gemini".to_string()),
        ],
    ) {
        // 排除包含分隔符的 task_text，防止格式冲突
        prop_assume!(!task_text.contains("---TASK---"));
        prop_assume!(!task_text.contains("---CONTENT---"));

        let specs = vec![TaskSpec {
            id: id.clone(),
            task_text: task_text.clone(),
            workdir: None,
            backend: backend.clone(),
            dependencies: vec![],
            session_id: None,
        }];

        let serialized = serialize_task_format(&specs);
        let parsed = parse_task_format(&serialized).unwrap();

        prop_assert_eq!(parsed.len(), 1);
        prop_assert_eq!(&parsed[0].id, &id);
        prop_assert_eq!(&parsed[0].backend, &backend);
        // 内容在 trim 后应与原始一致（序列化可能带换行）
        prop_assert_eq!(parsed[0].task_text.trim(), task_text.trim());
    }
}
