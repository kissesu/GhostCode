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
/// 使用 dirs::home_dir() 获取用户主目录，
/// 不可用时返回 None（调用方应跳过写入）
fn groups_base_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ghostcode").join("groups"))
}

/// 校验 group_id 是否合法（防路径穿越，二次防御）
///
/// 安全规则（与 ghostcode-daemon 侧 validate_group_id 对齐）：
/// 1. 不允许为空
/// 2. 仅允许字母、数字、连字符、下划线
///
/// @param group_id - 待校验的 Group ID
/// @returns 合法返回 true
fn is_valid_group_id(group_id: &str) -> bool {
    !group_id.is_empty()
        && group_id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// 构造账本文件路径
///
/// 格式: ~/.ghostcode/groups/{group_id}/state/ledger/ledger.jsonl
/// 返回 None 表示路径不可用（HOME 缺失或 group_id 非法）
fn ledger_path(group_id: &str) -> Option<PathBuf> {
    if !is_valid_group_id(group_id) {
        return None;
    }
    groups_base_dir().map(|base| base.join(group_id).join("state/ledger/ledger.jsonl"))
}

/// 构造账本锁文件路径
///
/// 格式: ~/.ghostcode/groups/{group_id}/state/ledger/ledger.lock
/// 返回 None 表示路径不可用
fn lock_path(group_id: &str) -> Option<PathBuf> {
    if !is_valid_group_id(group_id) {
        return None;
    }
    groups_base_dir().map(|base| base.join(group_id).join("state/ledger/ledger.lock"))
}

// ============================================
// 公共 API
// ============================================

/// Best-effort 写入事件到账本
///
/// 业务逻辑：
/// 1. 校验 group_id 合法性（二次防御，防路径穿越）
/// 2. 构造账本文件路径和锁文件路径
/// 3. 确保账本目录存在（不存在则创建）
/// 4. 调用 ghostcode_ledger::append_event 写入
/// 5. 任何失败仅 warn 日志，不 panic，不阻塞主流程
///
/// @param group_id - 所属 Group 标识，用于构造路径
/// @param event - 要写入的事件
/// @returns true 表示写入成功，false 表示跳过或失败
pub fn try_write_event(group_id: &str, event: &Event) -> bool {
    // C1 安全修复：二次校验 group_id，防止路径穿越
    let ledger = match ledger_path(group_id) {
        Some(p) => p,
        None => {
            tracing::warn!(
                "[wrapper-ledger] group_id 无效或 HOME 不可用，跳过写入: {:?}",
                group_id
            );
            return false;
        }
    };
    let lock = match lock_path(group_id) {
        Some(p) => p,
        None => return false,
    };

    // 确保目录存在，失败则 warn 后返回
    if let Some(parent) = ledger.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!(
                "[wrapper-ledger] 创建账本目录失败: {} - {}",
                parent.display(),
                e
            );
            return false;
        }
    }

    if let Err(e) = ghostcode_ledger::append_event(&ledger, &lock, event) {
        tracing::warn!(
            "[wrapper-ledger] 事件写入失败: {:?} - {}",
            event.kind,
            e
        );
        return false;
    }

    true
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
