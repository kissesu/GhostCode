//! Dashboard 查询层 PBT 属性测试
//!
//! 验证时间线顺序单调性、快照幂等性等不变量
//!
//! @author Atlas.oi
//! @date 2026-03-03

use ghostcode_ledger::query::timeline_page;
use ghostcode_types::event::{Event, EventKind};
use proptest::prelude::*;
use std::path::PathBuf;
use tempfile::TempDir;

/// 向指定路径写入 n 个测试事件
fn write_events(path: &PathBuf, lock_path: &PathBuf, n: usize) {
    for i in 0..n {
        let event = Event::new(
            EventKind::ChatMessage,
            "group-pbt",
            format!("scope-{i}"),
            "actor-pbt",
            serde_json::json!({ "i": i }),
        );
        ghostcode_ledger::append_event(path, lock_path, &event).unwrap();
    }
}

proptest! {
    /// 时间线顺序单调性：items 按 ts 倒序排列（最新事件在前）
    /// 上一轮修复将 timeline_page 改为倒序返回，与 Dashboard 显示方向一致
    #[test]
    fn timeline_order_monotonic(n in 1usize..20) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ledger.ndjson");
        let lock_path = dir.path().join("ledger.lock");
        write_events(&path, &lock_path, n);
        let page = timeline_page(&path, n + 10, None).unwrap();
        let tss: Vec<&str> = page.items.iter().map(|i| i.ts.as_str()).collect();
        let mut sorted_desc = tss.clone();
        sorted_desc.sort_by(|a, b| b.cmp(a));
        prop_assert_eq!(tss, sorted_desc, "时间线应按 ts 倒序排列（最新在前）");
    }

    /// 分页完整性：遍历所有分页，汇总 item 数 == 账本事件总数
    #[test]
    fn pagination_completeness(n in 1usize..30, page_size in 1usize..10) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ledger.ndjson");
        let lock_path = dir.path().join("ledger.lock");
        write_events(&path, &lock_path, n);

        let mut collected = 0usize;
        let mut cursor = None;
        loop {
            let page = timeline_page(&path, page_size, cursor).unwrap();
            collected += page.items.len();
            cursor = page.next_cursor;
            if cursor.is_none() { break; }
        }
        prop_assert_eq!(collected, n, "所有分页汇总数应等于事件总数");
    }

    /// append 后快照不回退：追加后 total_events 只增不减
    #[test]
    fn snapshot_total_monotonic(n in 1usize..15) {
        use ghostcode_ledger::query::build_history_projection;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ledger.ndjson");
        let lock_path = dir.path().join("ledger.lock");
        write_events(&path, &lock_path, n);
        let s1 = build_history_projection(&path, "g").unwrap();
        // 再追加一个事件
        let extra = Event::new(EventKind::ChatMessage, "g", "s", "a", serde_json::json!({}));
        ghostcode_ledger::append_event(&path, &lock_path, &extra).unwrap();
        let s2 = build_history_projection(&path, "g").unwrap();
        prop_assert!(s2.total_events >= s1.total_events, "追加后 total 不应减少");
    }
}
