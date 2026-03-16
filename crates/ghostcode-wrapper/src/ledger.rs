//! @file ledger.rs
//! @description Wrapper 账本写入模块
//!              Best-effort 写入：失败时仅记录警告，不阻塞模型执行。
//!              提供 RouteStart / RouteComplete / RouteError 事件构造与写入功能。
//! @author Atlas.oi
//! @date 2026-03-16

use std::path::PathBuf;

use ghostcode_types::event::{Event, EventKind};

// ============================================
// 内部路径辅助函数
// ============================================

/// 获取 GhostCode groups 目录的基础路径
///
/// 优先使用 HOME 环境变量，回退到 "." 避免 panic
fn groups_base_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".ghostcode").join("groups")
}

/// 构造账本文件路径
///
/// 格式: ~/.ghostcode/groups/{group_id}/state/ledger/ledger.jsonl
fn ledger_path(group_id: &str) -> PathBuf {
    groups_base_dir()
        .join(group_id)
        .join("state/ledger/ledger.jsonl")
}

/// 构造账本锁文件路径
///
/// 格式: ~/.ghostcode/groups/{group_id}/state/ledger/ledger.lock
fn lock_path(group_id: &str) -> PathBuf {
    groups_base_dir()
        .join(group_id)
        .join("state/ledger/ledger.lock")
}

// ============================================
// 公共 API
// ============================================

/// Best-effort 写入事件到账本
///
/// 业务逻辑：
/// 1. 构造账本文件路径和锁文件路径
/// 2. 确保账本目录存在（不存在则创建）
/// 3. 调用 ghostcode_ledger::append_event 写入
/// 4. 任何失败仅 warn 日志，不 panic，不阻塞主流程
///
/// @param group_id - 所属 Group 标识，用于构造路径
/// @param event - 要写入的事件
pub fn try_write_event(group_id: &str, event: &Event) {
    let ledger = ledger_path(group_id);
    let lock = lock_path(group_id);

    // 确保目录存在，失败则 warn 后返回
    if let Some(parent) = ledger.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!(
                "[wrapper-ledger] 创建账本目录失败: {} - {}",
                parent.display(),
                e
            );
            return;
        }
    }

    if let Err(e) = ghostcode_ledger::append_event(&ledger, &lock, event) {
        tracing::warn!(
            "[wrapper-ledger] 事件写入失败: {:?} - {}",
            event.kind,
            e
        );
    }
}

/// 构造 RouteStart 事件
///
/// @param group_id - 所属 Group 标识
/// @param correlation_id - 贯穿 start/complete/error 的关联 ID
/// @param backend - 后端名称（codex / claude / gemini）
/// @param task_summary - 任务文本摘要（前 200 字符）
pub fn route_start_event(
    group_id: &str,
    correlation_id: &str,
    backend: &str,
    task_summary: &str,
) -> Event {
    Event::new(
        EventKind::RouteStart,
        group_id,
        "ghostcode-wrapper",
        "user",
        serde_json::json!({
            "correlation_id": correlation_id,
            "backend": backend,
            "task_summary": task_summary,
        }),
    )
}

/// 构造 RouteComplete 事件
///
/// @param group_id - 所属 Group 标识
/// @param correlation_id - 与 RouteStart 相同的关联 ID
/// @param backend - 后端名称
/// @param duration_ms - 模型调用耗时（毫秒）
/// @param output_summary - 输出文本摘要（可选，前 200 字符）
pub fn route_complete_event(
    group_id: &str,
    correlation_id: &str,
    backend: &str,
    duration_ms: u64,
    output_summary: Option<&str>,
) -> Event {
    Event::new(
        EventKind::RouteComplete,
        group_id,
        "ghostcode-wrapper",
        "user",
        serde_json::json!({
            "correlation_id": correlation_id,
            "backend": backend,
            "duration_ms": duration_ms,
            "output_summary": output_summary,
        }),
    )
}

/// 构造 RouteError 事件
///
/// @param group_id - 所属 Group 标识
/// @param correlation_id - 与 RouteStart 相同的关联 ID
/// @param backend - 后端名称
/// @param duration_ms - 模型调用耗时（毫秒）
/// @param error_message - 错误描述
pub fn route_error_event(
    group_id: &str,
    correlation_id: &str,
    backend: &str,
    duration_ms: u64,
    error_message: &str,
) -> Event {
    Event::new(
        EventKind::RouteError,
        group_id,
        "ghostcode-wrapper",
        "user",
        serde_json::json!({
            "correlation_id": correlation_id,
            "backend": backend,
            "duration_ms": duration_ms,
            "error_message": error_message,
        }),
    )
}
