//! Actor 注册/发现/移除
//!
//! 管理 Group 内的 Actor（Agent）成员
//! 支持添加、查找、移除操作，并持久化到 group.yaml 和事件账本
//!
//! 参考: cccc/src/cccc/kernel/actors.py
//!
//! @author Atlas.oi
//! @date 2026-03-01

use std::path::Path;

use ghostcode_ledger::append_event;
use ghostcode_types::actor::{ActorInfo, ActorRole, RuntimeKind};
use ghostcode_types::event::{Event, EventKind};
use ghostcode_types::group::GroupInfo;

/// Actor 管理错误类型
#[derive(Debug, thiserror::Error)]
pub enum ActorError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML 错误: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("账本错误: {0}")]
    Ledger(#[from] ghostcode_ledger::LedgerError),

    #[error("Actor ID 重复: {0}")]
    DuplicateId(String),

    #[error("Group 已有 Foreman，不允许添加第二个")]
    DuplicateForeman,

    #[error("Actor 不存在: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, ActorError>;

/// 生成 Actor ID
///
/// 格式: <prefix>-<runtime>-<4位hex>
/// 例: dev-claude-a1b2
///
/// @param prefix - 前缀（如 "dev", "build"）
/// @param runtime - 运行时类型
pub fn generate_actor_id(prefix: &str, runtime: &RuntimeKind) -> String {
    let runtime_str = match runtime {
        RuntimeKind::Claude => "claude",
        RuntimeKind::Codex => "codex",
        RuntimeKind::Gemini => "gemini",
        RuntimeKind::Custom(name) => name.as_str(),
    };
    let hex = &uuid::Uuid::new_v4().simple().to_string()[..4];
    format!("{}-{}-{}", prefix, runtime_str, hex)
}

/// 添加 Actor 到 Group
///
/// 写入 ActorAdd 事件，更新 group.yaml
///
/// 约束：
/// - 不允许重复 actor_id
/// - 每个 Group 最多 1 个 Foreman
///
/// @param groups_dir - groups 根目录
/// @param group - 要修改的 GroupInfo（原地更新）
/// @param actor - 要添加的 ActorInfo
pub fn add_actor(
    groups_dir: &Path,
    group: &mut GroupInfo,
    actor: ActorInfo,
) -> Result<()> {
    // 检查 actor_id 重复
    if group.actors.iter().any(|a| a.actor_id == actor.actor_id) {
        return Err(ActorError::DuplicateId(actor.actor_id));
    }

    // 检查 Foreman 唯一性
    if actor.role == ActorRole::Foreman && find_foreman(group).is_some() {
        return Err(ActorError::DuplicateForeman);
    }

    let group_dir = groups_dir.join(&group.group_id);

    // 写入 ActorAdd 事件
    let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
    let lock_path = group_dir.join("state/ledger/ledger.lock");
    let event = Event::new(
        EventKind::ActorAdd,
        &group.group_id,
        "default",
        "user",
        serde_json::json!({
            "actor_id": actor.actor_id,
            "display_name": actor.display_name,
            "role": actor.role,
            "runtime": actor.runtime,
        }),
    );
    append_event(&ledger_path, &lock_path, &event)?;

    // 更新内存中的 group
    group.actors.push(actor);

    // 持久化到 group.yaml
    persist_group_yaml(&group_dir, group)?;

    Ok(())
}

/// 查找 Actor
///
/// @param group - Group 信息
/// @param actor_id - Actor ID
/// @return 找到的 ActorInfo 引用，或 None
pub fn find_actor<'a>(group: &'a GroupInfo, actor_id: &str) -> Option<&'a ActorInfo> {
    group.actors.iter().find(|a| a.actor_id == actor_id)
}

/// 查找 Foreman（role == Foreman 的 Actor）
///
/// @param group - Group 信息
/// @return Foreman 的 ActorInfo 引用，或 None
pub fn find_foreman(group: &GroupInfo) -> Option<&ActorInfo> {
    group.actors.iter().find(|a| a.role == ActorRole::Foreman)
}

/// 列出所有 Actor
///
/// @param group - Group 信息
/// @return Actor 列表切片
pub fn list_actors(group: &GroupInfo) -> &[ActorInfo] {
    &group.actors
}

/// 移除 Actor
///
/// 写入 ActorRemove 事件，更新 group.yaml
///
/// @param groups_dir - groups 根目录
/// @param group - 要修改的 GroupInfo（原地更新）
/// @param actor_id - 要移除的 Actor ID
pub fn remove_actor(
    groups_dir: &Path,
    group: &mut GroupInfo,
    actor_id: &str,
) -> Result<()> {
    // 确认 actor 存在
    if !group.actors.iter().any(|a| a.actor_id == actor_id) {
        return Err(ActorError::NotFound(actor_id.to_string()));
    }

    let group_dir = groups_dir.join(&group.group_id);

    // 写入 ActorRemove 事件
    let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
    let lock_path = group_dir.join("state/ledger/ledger.lock");
    let event = Event::new(
        EventKind::ActorRemove,
        &group.group_id,
        "default",
        "user",
        serde_json::json!({ "actor_id": actor_id }),
    );
    append_event(&ledger_path, &lock_path, &event)?;

    // 从内存中移除
    group.actors.retain(|a| a.actor_id != actor_id);

    // 持久化
    persist_group_yaml(&group_dir, group)?;

    Ok(())
}

// ============================================
// 内部辅助
// ============================================

/// 持久化 group.yaml
fn persist_group_yaml(group_dir: &Path, group: &GroupInfo) -> Result<()> {
    let yaml_path = group_dir.join("group.yaml");
    let yaml = serde_yaml::to_string(group)?;
    std::fs::write(&yaml_path, yaml)?;
    Ok(())
}
