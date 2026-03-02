//! ghostcode-ledger 集成测试
//!
//! 覆盖 T03 TDD 规范定义的所有测试用例
//! - 追加写入与计数
//! - 反向读取
//! - 全量迭代
//! - PBT 单调性和往返性
//! - 并发原子性
//! - 损坏行容错 [ERR-1]
//!
//! @author Atlas.oi
//! @date 2026-03-01

use std::io::Write;
use std::sync::{Arc, Barrier};
use std::thread;

use ghostcode_ledger::{append_event, count_events, iter_events, read_last_lines};
use ghostcode_types::event::{Event, EventKind};
use ghostcode_types::testutil::arb_event;
use proptest::prelude::*;
use tempfile::TempDir;

/// 创建测试用的临时账本环境
fn setup() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let ledger = dir.path().join("ledger.jsonl");
    let lock = dir.path().join("ledger.lock");
    (dir, ledger, lock)
}

/// 创建一个简单的测试事件
fn make_event(kind: EventKind) -> Event {
    Event::new(kind, "test-group", "default", "user", serde_json::json!({}))
}

// ============================================
// 单元测试
// ============================================

#[test]
fn append_and_count() {
    let (_dir, ledger, lock) = setup();

    // 追加 10 个事件
    for _ in 0..10 {
        append_event(&ledger, &lock, &make_event(EventKind::ChatMessage)).unwrap();
    }

    assert_eq!(count_events(&ledger).unwrap(), 10);
}

#[test]
fn append_and_read_last() {
    let (_dir, ledger, lock) = setup();

    // 追加 5 个不同类型的事件
    let kinds = vec![
        EventKind::GroupCreate,
        EventKind::ActorAdd,
        EventKind::ChatMessage,
        EventKind::ChatRead,
        EventKind::SystemNotify,
    ];
    let mut events = Vec::new();
    for kind in kinds {
        let event = make_event(kind);
        append_event(&ledger, &lock, &event).unwrap();
        events.push(event);
    }

    // 读取最后 3 个
    let last3 = read_last_lines(&ledger, 3).unwrap();
    assert_eq!(last3.len(), 3);

    // 验证是最后 3 个事件（按 kind 比较，因为 id/ts 每次不同）
    assert_eq!(last3[0].kind, EventKind::ChatMessage);
    assert_eq!(last3[1].kind, EventKind::ChatRead);
    assert_eq!(last3[2].kind, EventKind::SystemNotify);
}

#[test]
fn iter_all_events() {
    let (_dir, ledger, lock) = setup();

    let n = 20;
    let mut written_kinds = Vec::new();
    for i in 0..n {
        let kind = if i % 2 == 0 {
            EventKind::ChatMessage
        } else {
            EventKind::ActorAdd
        };
        append_event(&ledger, &lock, &make_event(kind.clone())).unwrap();
        written_kinds.push(kind);
    }

    // 迭代收集所有事件
    let all: Vec<Event> = iter_events(&ledger)
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    assert_eq!(all.len(), n);

    // 验证顺序正确
    for (i, event) in all.iter().enumerate() {
        assert_eq!(event.kind, written_kinds[i], "第 {} 个事件类型不匹配", i);
    }
}

#[test]
fn empty_ledger_returns_zero() {
    let (_dir, ledger, _lock) = setup();

    // 文件不存在时
    assert_eq!(count_events(&ledger).unwrap(), 0);

    // 创建空文件
    std::fs::File::create(&ledger).unwrap();
    assert_eq!(count_events(&ledger).unwrap(), 0);
}

#[test]
fn corrupted_line_skipped() {
    // [ERR-1] 手动写入非 JSON 行，iter_events 应跳过该行不 panic
    let (_dir, ledger, lock) = setup();

    // 先追加一个合法事件
    append_event(&ledger, &lock, &make_event(EventKind::GroupCreate)).unwrap();

    // 手动写入一行垃圾数据
    {
        let mut f = std::fs::OpenOptions::new().append(true).open(&ledger).unwrap();
        writeln!(f, "this is not valid json!!!").unwrap();
    }

    // 再追加一个合法事件
    append_event(&ledger, &lock, &make_event(EventKind::ChatMessage)).unwrap();

    // iter_events 应该只返回 2 个合法事件，跳过损坏行
    let events: Vec<Event> = iter_events(&ledger)
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    assert_eq!(events.len(), 2);
    assert_eq!(events[0].kind, EventKind::GroupCreate);
    assert_eq!(events[1].kind, EventKind::ChatMessage);
}

#[test]
fn concurrent_append_atomicity() {
    let (_dir, ledger, lock) = setup();

    let ledger = Arc::new(ledger);
    let lock = Arc::new(lock);

    // 100 线程各追加 10 事件
    let thread_count = 100;
    let events_per_thread = 10;
    let barrier = Arc::new(Barrier::new(thread_count));

    let handles: Vec<_> = (0..thread_count)
        .map(|_| {
            let ledger = Arc::clone(&ledger);
            let lock = Arc::clone(&lock);
            let barrier = Arc::clone(&barrier);

            thread::spawn(move || {
                barrier.wait();
                for _ in 0..events_per_thread {
                    append_event(&ledger, &lock, &make_event(EventKind::ChatMessage)).unwrap();
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(
        count_events(&ledger).unwrap(),
        thread_count * events_per_thread
    );
}

// ============================================
// PBT 属性测试
// ============================================

proptest! {
    /// PBT: 单调性 — 追加 n 个事件后 count == n
    #[test]
    fn append_count_monotonic(n in 1..50usize) {
        let (_dir, ledger, lock) = setup();

        for _ in 0..n {
            append_event(&ledger, &lock, &make_event(EventKind::ChatMessage)).unwrap();
        }

        prop_assert_eq!(count_events(&ledger).unwrap(), n);
    }

    /// PBT: 往返性 — 写入事件 E → read_last_lines(1) → 等于 E
    #[test]
    fn append_roundtrip(event in arb_event()) {
        let (_dir, ledger, lock) = setup();

        append_event(&ledger, &lock, &event).unwrap();

        let last = read_last_lines(&ledger, 1).unwrap();
        prop_assert_eq!(last.len(), 1);

        let restored = &last[0];
        prop_assert_eq!(&event.v, &restored.v);
        prop_assert_eq!(&event.id, &restored.id);
        prop_assert_eq!(&event.ts, &restored.ts);
        prop_assert_eq!(&event.kind, &restored.kind);
        prop_assert_eq!(&event.group_id, &restored.group_id);
        prop_assert_eq!(&event.scope_key, &restored.scope_key);
        prop_assert_eq!(&event.by, &restored.by);
        prop_assert_eq!(&event.data, &restored.data);
    }
}
