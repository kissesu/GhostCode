//! ghostcode-web SSE 流式接口测试
//!
//! 验证 SSE endpoint 建立连接并推送事件
//!
//! @author Atlas.oi
//! @date 2026-03-03

use ghostcode_ledger::append_event;
use ghostcode_types::event::{Event, EventKind};
use ghostcode_web::sse::tail_ledger_as_sse;
use tempfile::TempDir;
use tokio_stream::StreamExt;

#[tokio::test]
async fn sse_tail_existing_ledger() {
    let dir = TempDir::new().unwrap();
    let ledger = dir.path().join("ledger.ndjson");
    let lock = dir.path().join("ledger.lock");

    // 先写入一个事件
    let event = Event::new(
        EventKind::ChatMessage,
        "g1",
        "s1",
        "actor-1",
        serde_json::json!({"text": "hello"}),
    );
    append_event(&ledger, &lock, &event).unwrap();

    // SSE tail 应该能读取到已有的事件（从头开始读）
    // 使用 take(1) 只取第一个事件，避免无限等待
    let stream = tail_ledger_as_sse(ledger, true);
    tokio::pin!(stream);

    // 给 5 秒超时，防止测试卡住
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        stream.next(),
    )
    .await;

    assert!(result.is_ok(), "SSE stream 应在 5 秒内产生输出");
}

#[tokio::test]
async fn sse_tail_nonexistent_file_no_panic() {
    let dir = TempDir::new().unwrap();
    let ledger = dir.path().join("nonexistent.ndjson");

    // 文件不存在时 tail 应不 panic，等待文件出现
    let stream = tail_ledger_as_sse(ledger, false);
    tokio::pin!(stream);

    // 200ms 内不应 panic（无事件推送也可以）
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(200),
        stream.next(),
    )
    .await;

    // timeout 或 None 都可以，关键是不 panic
    let _ = result;
}
