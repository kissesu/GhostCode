//! ghostcode-ledger blob 溢出测试
//!
//! 覆盖 T04 TDD 规范定义的所有测试用例
//! - 小消息不产生 blob
//! - 大消息溢出到 blob 文件
//! - blob 往返读取
//! - PBT 阈值边界和内容完整性
//! - 幂等性
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_ledger::blob::{maybe_spill_blob, read_blob, BLOB_THRESHOLD};
use ghostcode_types::event::EventKind;
use proptest::prelude::*;
use tempfile::TempDir;

/// 创建测试用的临时 blob 目录
fn setup() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let blobs_dir = dir.path().join("blobs");
    (dir, blobs_dir)
}

/// 创建指定大小的 body 字符串
fn make_body(size: usize) -> String {
    "a".repeat(size)
}

// ============================================
// 单元测试
// ============================================

#[test]
fn small_message_no_blob() {
    let (_dir, blobs_dir) = setup();

    // 1KB body，不应产生 blob 文件
    let body = make_body(1024);
    let data = serde_json::json!({ "text": body });

    let result = maybe_spill_blob(&blobs_dir, "test-id-001", &EventKind::ChatMessage, &data).unwrap();

    // data 应保持不变
    assert_eq!(result, data);

    // blobs 目录不应被创建（因为没有溢出）
    assert!(!blobs_dir.exists());
}

#[test]
fn large_message_spills_blob() {
    let (_dir, blobs_dir) = setup();

    // 50KB body，应产生 blob 文件
    let body = make_body(50 * 1024);
    let data = serde_json::json!({ "text": body });

    let result = maybe_spill_blob(&blobs_dir, "test-id-002", &EventKind::ChatMessage, &data).unwrap();

    // blob 文件应存在
    let blob_path = blobs_dir.join("chat.test-id-002.txt");
    assert!(blob_path.exists(), "blob 文件应已创建");

    // result 应包含 _blob_ref 和 body_preview
    assert!(result.get("_blob_ref").is_some(), "应包含 _blob_ref");
    assert!(result.get("body_preview").is_some(), "应包含 body_preview");
    assert!(result.get("text").is_none(), "原始 text 应被移除");

    // _blob_ref 应为正确的文件名
    assert_eq!(
        result["_blob_ref"].as_str().unwrap(),
        "chat.test-id-002.txt"
    );

    // body_preview 应为前 200 个字符
    let preview = result["body_preview"].as_str().unwrap();
    assert_eq!(preview.len(), 200);
}

#[test]
fn blob_roundtrip() {
    let (_dir, blobs_dir) = setup();

    // 50KB body
    let body = make_body(50 * 1024);
    let data = serde_json::json!({ "text": body });

    let result = maybe_spill_blob(&blobs_dir, "test-id-003", &EventKind::ChatMessage, &data).unwrap();

    // 通过 read_blob 读取
    let blob_ref = result["_blob_ref"].as_str().unwrap();
    let restored = read_blob(&blobs_dir, blob_ref).unwrap();

    // 内容应完全相等
    assert_eq!(restored, body);
}

#[test]
fn spill_idempotent() {
    let (_dir, blobs_dir) = setup();

    let body = make_body(50 * 1024);
    let data = serde_json::json!({ "text": body });

    // 两次 spill 同一事件
    let result1 = maybe_spill_blob(&blobs_dir, "test-id-004", &EventKind::ChatMessage, &data).unwrap();
    let result2 = maybe_spill_blob(&blobs_dir, "test-id-004", &EventKind::ChatMessage, &data).unwrap();

    // 结果应一致
    assert_eq!(result1, result2);

    // blob 文件内容也应一致
    let content1 = read_blob(&blobs_dir, result1["_blob_ref"].as_str().unwrap()).unwrap();
    let content2 = read_blob(&blobs_dir, result2["_blob_ref"].as_str().unwrap()).unwrap();
    assert_eq!(content1, content2);
}

#[test]
fn non_chat_message_no_blob() {
    let (_dir, blobs_dir) = setup();

    // 50KB body 但类型不是 ChatMessage
    let body = make_body(50 * 1024);
    let data = serde_json::json!({ "text": body });

    let result = maybe_spill_blob(&blobs_dir, "test-id-005", &EventKind::SystemNotify, &data).unwrap();

    // data 应保持不变
    assert_eq!(result, data);
    assert!(!blobs_dir.exists());
}

// ============================================
// PBT 属性测试
// ============================================

proptest! {
    /// PBT: 阈值边界精确性
    /// 在 30000-35000 字节范围内，< 32768 无 blob，>= 32768 有 blob
    #[test]
    fn threshold_boundary(size in 30000..35000usize) {
        let (_dir, blobs_dir) = setup();
        let body = make_body(size);
        let data = serde_json::json!({ "text": body });

        let result = maybe_spill_blob(
            &blobs_dir,
            &format!("boundary-{}", size),
            &EventKind::ChatMessage,
            &data,
        ).unwrap();

        if size < BLOB_THRESHOLD {
            // 小于阈值：无 blob 文件
            prop_assert_eq!(result, data, "size={} 不应溢出", size);
        } else {
            // 大于等于阈值：有 blob 文件
            prop_assert!(result.get("_blob_ref").is_some(), "size={} 应溢出", size);
            let blob_path = blobs_dir.join(result["_blob_ref"].as_str().unwrap());
            prop_assert!(blob_path.exists(), "size={} blob 文件应存在", size);
        }
    }

    /// PBT: blob 内容完整性（含特殊字符）[补充 PBT]
    /// 溢出后读回的内容应逐字节相等
    #[test]
    fn blob_content_integrity(body in ".{33000,50000}") {
        let (_dir, blobs_dir) = setup();
        let data = serde_json::json!({ "text": body });

        let result = maybe_spill_blob(
            &blobs_dir,
            "integrity-test",
            &EventKind::ChatMessage,
            &data,
        ).unwrap();

        // 应溢出
        let blob_ref = result["_blob_ref"].as_str().unwrap();
        let restored = read_blob(&blobs_dir, blob_ref).unwrap();

        prop_assert_eq!(restored, body, "溢出后读回内容应完全相等");
    }
}
