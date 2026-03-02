//! Group 数据模型 + 持久化
//!
//! Group 是 Agent 协作的容器单元
//! 每个 Group 有独立的目录结构（ledger + blobs + cursors）
//!
//! 参考: cccc/src/cccc/kernel/group.py
//!
//! @author Atlas.oi
//! @date 2026-03-01

use std::fs;
use std::path::Path;

use ghostcode_ledger::append_event;
use ghostcode_types::event::{Event, EventKind};
use ghostcode_types::group::{GroupInfo, GroupState};

/// Group 管理错误类型
#[derive(Debug, thiserror::Error)]
pub enum GroupError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML 错误: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("账本错误: {0}")]
    Ledger(#[from] ghostcode_ledger::LedgerError),

    #[error("Group 不存在: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, GroupError>;

/// 生成 group_id
///
/// 格式: g-<8位随机hex>
fn generate_group_id() -> String {
    let hex = &uuid::Uuid::new_v4().simple().to_string()[..8];
    format!("g-{}", hex)
}

/// 创建 Group
///
/// 生成 group_id，创建完整目录结构，写入 group.yaml，写入 GroupCreate 事件
///
/// 目录结构：
/// groups/<group_id>/
///   group.yaml
///   state/ledger/ledger.jsonl
///   state/ledger/ledger.lock
///   state/ledger/blobs/
///   state/read_cursors.json
///
/// @param groups_dir - groups 根目录（如 ~/.ghostcode/groups/）
/// @param title - Group 标题
/// @return 创建的 GroupInfo
pub fn create_group(groups_dir: &Path, title: &str) -> Result<GroupInfo> {
    let group_id = generate_group_id();
    let group_dir = groups_dir.join(&group_id);

    // 创建完整目录结构
    fs::create_dir_all(group_dir.join("state/ledger/blobs"))?;

    // 初始化空的 read_cursors.json
    fs::write(
        group_dir.join("state/read_cursors.json"),
        "{}",
    )?;

    let group = GroupInfo {
        group_id: group_id.clone(),
        title: title.to_string(),
        state: GroupState::Idle,
        actors: Vec::new(),
    };

    // 写入 group.yaml
    write_group_yaml(&group_dir, &group)?;

    // 写入 GroupCreate 事件到账本
    let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
    let lock_path = group_dir.join("state/ledger/ledger.lock");
    let event = Event::new(
        EventKind::GroupCreate,
        &group_id,
        "default",
        "user",
        serde_json::json!({ "title": title }),
    );
    append_event(&ledger_path, &lock_path, &event)?;

    Ok(group)
}

/// 加载 Group（从 group.yaml 读取）
///
/// @param group_dir - Group 目录路径
/// @return GroupInfo
pub fn load_group(group_dir: &Path) -> Result<GroupInfo> {
    let yaml_path = group_dir.join("group.yaml");
    if !yaml_path.exists() {
        return Err(GroupError::NotFound(
            group_dir.to_string_lossy().to_string(),
        ));
    }
    let content = fs::read_to_string(&yaml_path)?;
    let group: GroupInfo = serde_yaml::from_str(&content)?;
    Ok(group)
}

/// 列出所有 Group
///
/// 扫描 groups_dir 下的所有子目录，尝试加载 group.yaml
///
/// @param groups_dir - groups 根目录
/// @return GroupInfo 列表
pub fn list_groups(groups_dir: &Path) -> Result<Vec<GroupInfo>> {
    if !groups_dir.exists() {
        return Ok(Vec::new());
    }

    let mut groups = Vec::new();
    for entry in fs::read_dir(groups_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            match load_group(&entry.path()) {
                Ok(group) => groups.push(group),
                Err(_) => continue, // 跳过无效目录
            }
        }
    }

    Ok(groups)
}

/// 删除 Group（清理整个目录）
///
/// @param groups_dir - groups 根目录
/// @param group_id - 要删除的 Group ID
pub fn delete_group(groups_dir: &Path, group_id: &str) -> Result<()> {
    let group_dir = groups_dir.join(group_id);
    if !group_dir.exists() {
        return Err(GroupError::NotFound(group_id.to_string()));
    }
    fs::remove_dir_all(&group_dir)?;
    Ok(())
}

/// 设置 Group 状态
///
/// 同时更新 group.yaml 和写入 GroupSetState 事件
///
/// @param groups_dir - groups 根目录
/// @param group - 要修改的 GroupInfo（会被原地更新）
/// @param state - 新状态
pub fn set_group_state(
    groups_dir: &Path,
    group: &mut GroupInfo,
    state: GroupState,
) -> Result<()> {
    let group_dir = groups_dir.join(&group.group_id);

    group.state = state.clone();

    // 更新 group.yaml
    write_group_yaml(&group_dir, group)?;

    // 写入 GroupSetState 事件
    let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
    let lock_path = group_dir.join("state/ledger/ledger.lock");
    let event = Event::new(
        EventKind::GroupSetState,
        &group.group_id,
        "default",
        "user",
        serde_json::json!({ "state": state }),
    );
    append_event(&ledger_path, &lock_path, &event)?;

    Ok(())
}

// ============================================
// 内部辅助：YAML 持久化
// ============================================

/// 写入 group.yaml
fn write_group_yaml(group_dir: &Path, group: &GroupInfo) -> Result<()> {
    let yaml_path = group_dir.join("group.yaml");
    let yaml = serde_yaml::to_string(group)?;
    fs::write(&yaml_path, yaml)?;
    Ok(())
}
