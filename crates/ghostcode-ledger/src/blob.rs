//! Blob 溢出处理
//!
//! 当 ChatMessage 的 data.text 超过 32KB 阈值时
//! 将完整内容溢出到独立 blob 文件，账本中保留引用和预览
//!
//! 参考: cccc/src/cccc/kernel/ledger.py blob 逻辑 + kernel/blobs.py
//!
//! @author Atlas.oi
//! @date 2026-03-01

use std::fs;
use std::path::Path;

use ghostcode_types::event::EventKind;

use crate::Result;

/// 溢出阈值：32KB
pub const BLOB_THRESHOLD: usize = 32 * 1024;

/// body 预览长度（保留在账本 JSON 中的前缀字符数）
const PREVIEW_LEN: usize = 200;

/// 检查 data 是否需要溢出，如需则写入 blob 文件并返回引用
///
/// 仅 ChatMessage 类型检查溢出（data.text 字段）
/// 溢出文件路径：blobs_dir/chat.<event_id>.txt
/// 溢出后 data 中保留 { "_blob_ref": "chat.<event_id>.txt", "body_preview": "<前200字符>" }
///
/// 非 ChatMessage 类型或 body 小于阈值时，返回原始 data 不做修改
///
/// @param blobs_dir - blob 文件存储目录
/// @param event_id - 事件 ID（用于生成 blob 文件名）
/// @param kind - 事件类型（仅 ChatMessage 触发溢出）
/// @param data - 事件负载数据
/// @return 可能被替换的 data（含 blob 引用）
pub fn maybe_spill_blob(
    blobs_dir: &Path,
    event_id: &str,
    kind: &EventKind,
    data: &serde_json::Value,
) -> Result<serde_json::Value> {
    // 仅 ChatMessage 类型检查溢出
    if *kind != EventKind::ChatMessage {
        return Ok(data.clone());
    }

    // 提取 text 字段（与 CCCC ChatMessageData.text 保持一致）
    let body = match data.get("text").and_then(|v| v.as_str()) {
        Some(b) => b,
        None => return Ok(data.clone()),
    };

    // 检查是否超过阈值
    if body.len() < BLOB_THRESHOLD {
        return Ok(data.clone());
    }

    // 确保 blobs 目录存在
    fs::create_dir_all(blobs_dir)?;

    // 生成 blob 文件名和路径
    let blob_filename = format!("chat.{}.txt", event_id);
    let blob_path = blobs_dir.join(&blob_filename);

    // 写入 blob 文件
    fs::write(&blob_path, body)?;

    // 生成预览（取前 PREVIEW_LEN 个字符）
    let preview: String = body.chars().take(PREVIEW_LEN).collect();

    // 构建替换后的 data，保留原 data 的其他字段
    let mut new_data = data.clone();
    if let Some(obj) = new_data.as_object_mut() {
        obj.remove("text");
        obj.insert(
            "_blob_ref".to_string(),
            serde_json::Value::String(blob_filename),
        );
        obj.insert(
            "body_preview".to_string(),
            serde_json::Value::String(preview),
        );
    }

    Ok(new_data)
}

/// 读取 blob 内容
///
/// @param blobs_dir - blob 文件存储目录
/// @param blob_ref - blob 文件名引用（如 "chat.<event_id>.txt"）
/// @return blob 文件的完整文本内容
pub fn read_blob(blobs_dir: &Path, blob_ref: &str) -> Result<String> {
    // 路径遍历防护：拒绝包含 / 、 \ 或 .. 的 blob_ref
    if blob_ref.contains('/') || blob_ref.contains('\\') || blob_ref.contains("..") || blob_ref.is_empty() {
        return Err(crate::LedgerError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("非法的 blob_ref: \"{}\"，不允许路径遍历字符", blob_ref),
        )));
    }

    let blob_path = blobs_dir.join(blob_ref);
    let content = fs::read_to_string(&blob_path)?;
    Ok(content)
}
