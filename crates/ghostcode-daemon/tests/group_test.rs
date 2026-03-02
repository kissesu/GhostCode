//! ghostcode-daemon Group 管理测试
//!
//! 覆盖 T08 TDD 规范定义的所有测试用例
//! - 创建和加载 Group
//! - 列出 Groups
//! - 删除 Group
//! - 设置 Group 状态
//! - PBT 往返性
//! - blob 目录清理 [AMB-3]
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_daemon::group::{
    create_group, delete_group, list_groups, load_group, set_group_state,
};
use ghostcode_types::group::GroupState;
use proptest::prelude::*;
use tempfile::TempDir;

fn setup() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let groups_dir = dir.path().join("groups");
    std::fs::create_dir_all(&groups_dir).unwrap();
    (dir, groups_dir)
}

// ============================================
// 单元测试
// ============================================

#[test]
fn create_and_load_group() {
    let (_dir, groups_dir) = setup();

    let group = create_group(&groups_dir, "Test Project").unwrap();
    assert_eq!(group.title, "Test Project");
    assert_eq!(group.state, GroupState::Idle);
    assert!(group.group_id.starts_with("g-"));
    assert_eq!(group.group_id.len(), 10); // "g-" + 8 hex

    // load 后数据一致
    let group_dir = groups_dir.join(&group.group_id);
    let loaded = load_group(&group_dir).unwrap();
    assert_eq!(loaded.title, group.title);
    assert_eq!(loaded.state, group.state);
    assert_eq!(loaded.group_id, group.group_id);
}

#[test]
fn list_groups_returns_all() {
    let (_dir, groups_dir) = setup();

    create_group(&groups_dir, "Project A").unwrap();
    create_group(&groups_dir, "Project B").unwrap();
    create_group(&groups_dir, "Project C").unwrap();

    let groups = list_groups(&groups_dir).unwrap();
    assert_eq!(groups.len(), 3);

    let titles: Vec<&str> = groups.iter().map(|g| g.title.as_str()).collect();
    assert!(titles.contains(&"Project A"));
    assert!(titles.contains(&"Project B"));
    assert!(titles.contains(&"Project C"));
}

#[test]
fn delete_group_removes_dir() {
    let (_dir, groups_dir) = setup();

    let group = create_group(&groups_dir, "To Delete").unwrap();
    let group_dir = groups_dir.join(&group.group_id);
    assert!(group_dir.exists());

    delete_group(&groups_dir, &group.group_id).unwrap();
    assert!(!group_dir.exists());
}

#[test]
fn set_group_state_updates() {
    let (_dir, groups_dir) = setup();

    let mut group = create_group(&groups_dir, "Stateful").unwrap();
    assert_eq!(group.state, GroupState::Idle);

    set_group_state(&groups_dir, &mut group, GroupState::Active).unwrap();
    assert_eq!(group.state, GroupState::Active);

    // reload 验证持久化
    let group_dir = groups_dir.join(&group.group_id);
    let loaded = load_group(&group_dir).unwrap();
    assert_eq!(loaded.state, GroupState::Active);
}

#[test]
fn delete_group_cleans_blobs() {
    // [AMB-3] 创建 Group → 写入 blob 文件 → delete → blobs 目录不存在
    let (_dir, groups_dir) = setup();

    let group = create_group(&groups_dir, "Blob Test").unwrap();
    let blobs_dir = groups_dir
        .join(&group.group_id)
        .join("state/ledger/blobs");

    // 写入假 blob 文件
    std::fs::write(blobs_dir.join("chat.test.txt"), "blob content").unwrap();
    assert!(blobs_dir.join("chat.test.txt").exists());

    // delete 应清理全部
    delete_group(&groups_dir, &group.group_id).unwrap();
    assert!(!blobs_dir.exists());
}

#[test]
fn create_group_directory_structure() {
    let (_dir, groups_dir) = setup();

    let group = create_group(&groups_dir, "Structure Check").unwrap();
    let group_dir = groups_dir.join(&group.group_id);

    // 验证完整目录结构
    assert!(group_dir.join("group.yaml").exists());
    assert!(group_dir.join("state/ledger/ledger.jsonl").exists());
    assert!(group_dir.join("state/ledger/ledger.lock").exists());
    assert!(group_dir.join("state/ledger/blobs").is_dir());
    assert!(group_dir.join("state/read_cursors.json").exists());
}

#[test]
fn list_empty_groups_dir() {
    let (_dir, groups_dir) = setup();
    let groups = list_groups(&groups_dir).unwrap();
    assert!(groups.is_empty());
}

// ============================================
// PBT 属性测试
// ============================================

proptest! {
    /// PBT: create → load → title 相等
    #[test]
    fn create_load_roundtrip(title in "[a-zA-Z ]{1,50}") {
        let (_dir, groups_dir) = setup();

        let group = create_group(&groups_dir, &title).unwrap();
        let group_dir = groups_dir.join(&group.group_id);
        let loaded = load_group(&group_dir).unwrap();

        prop_assert_eq!(loaded.title, title);
        prop_assert_eq!(loaded.state, GroupState::Idle);
    }
}
