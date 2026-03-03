//! Dashboard 账本查询层测试
//!
//! 验证 timeline_page / aggregate_agent_status / build_history_projection 函数
//!
//! @author Atlas.oi
//! @date 2026-03-03

use ghostcode_ledger::query::{aggregate_agent_status, build_history_projection, timeline_page};
use ghostcode_types::event::{Event, EventKind};
use std::path::PathBuf;
use tempfile::TempDir;

/// 创建测试用账本文件，写入 N 个测试事件
///
/// 每个事件使用 ChatMessage 类型，actor 按 i % 3 分配
/// lock 文件与账本同目录
fn make_test_ledger(n: usize) -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let ledger_path = dir.path().join("ledger.ndjson");
    let lock_path = dir.path().join("ledger.lock");
    for i in 0..n {
        let event = Event::new(
            EventKind::ChatMessage,
            "group-test",
            format!("scope-{i}"),
            format!("actor-{}", i % 3),
            serde_json::json!({ "text": format!("消息 {i}") }),
        );
        ghostcode_ledger::append_event(&ledger_path, &lock_path, &event).unwrap();
    }
    // 即使 n == 0，也创建空账本文件，确保 iter_events 不会因文件不存在而报错
    if n == 0 {
        std::fs::File::create(&ledger_path).unwrap();
    }
    (dir, ledger_path)
}

#[test]
fn timeline_page_returns_items() {
    let (_dir, path) = make_test_ledger(5);
    let page = timeline_page(&path, 10, None).unwrap();
    assert_eq!(page.items.len(), 5);
    assert_eq!(page.total, 5);
    assert!(page.next_cursor.is_none());
}

#[test]
fn timeline_page_paginates() {
    let (_dir, path) = make_test_ledger(10);
    let page1 = timeline_page(&path, 4, None).unwrap();
    assert_eq!(page1.items.len(), 4);
    assert!(page1.next_cursor.is_some());

    let page2 = timeline_page(&path, 4, page1.next_cursor).unwrap();
    assert_eq!(page2.items.len(), 4);
}

#[test]
fn timeline_empty_ledger_returns_empty() {
    let (_dir, path) = make_test_ledger(0);
    let page = timeline_page(&path, 10, None).unwrap();
    assert_eq!(page.items.len(), 0);
    assert_eq!(page.total, 0);
    assert!(page.next_cursor.is_none());
}

#[test]
fn aggregate_agent_status_counts_actors() {
    // 9 个事件，actor-0/1/2 各 3 次
    let (_dir, path) = make_test_ledger(9);
    let agents = aggregate_agent_status(&path).unwrap();
    // 至少 3 个不同 actor
    assert!(
        agents.len() >= 3,
        "应检测到至少 3 个 actor，实际: {}",
        agents.len()
    );
}

#[test]
fn aggregate_empty_ledger_returns_empty() {
    let (_dir, path) = make_test_ledger(0);
    let agents = aggregate_agent_status(&path).unwrap();
    assert!(agents.is_empty());
}

#[test]
fn build_history_projection_snapshot() {
    let (_dir, path) = make_test_ledger(5);
    let snap = build_history_projection(&path, "group-test").unwrap();
    assert_eq!(snap.group_id, "group-test");
    assert_eq!(snap.total_events, 5);
    assert!(!snap.snapshot_ts.is_empty());
}

#[test]
fn build_projection_idempotent() {
    let (_dir, path) = make_test_ledger(3);
    let s1 = build_history_projection(&path, "group-test").unwrap();
    let s2 = build_history_projection(&path, "group-test").unwrap();
    assert_eq!(s1.total_events, s2.total_events);
    assert_eq!(s1.group_id, s2.group_id);
}
