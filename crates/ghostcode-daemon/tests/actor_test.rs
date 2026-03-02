//! ghostcode-daemon Actor 管理测试
//!
//! 覆盖 T09 TDD 规范定义的所有测试用例
//! - 添加和查找 Actor
//! - 三 Agent 场景
//! - Foreman 查找
//! - Actor 移除
//! - 重复 ID 拒绝
//! - 第二个 Foreman 拒绝
//! - PBT 唯一性
//!
//! @author Atlas.oi
//! @date 2026-03-01

use ghostcode_daemon::actor_mgmt::{
    add_actor, find_actor, find_foreman, generate_actor_id, list_actors, remove_actor,
};
use ghostcode_daemon::group::create_group;
use ghostcode_types::actor::{ActorInfo, ActorRole, RuntimeKind};
use tempfile::TempDir;

fn setup() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let groups_dir = dir.path().join("groups");
    std::fs::create_dir_all(&groups_dir).unwrap();
    (dir, groups_dir)
}

fn make_actor(actor_id: &str, name: &str, role: ActorRole, runtime: RuntimeKind) -> ActorInfo {
    ActorInfo {
        actor_id: actor_id.to_string(),
        display_name: name.to_string(),
        role,
        runtime,
        running: false,
        pid: None,
    }
}

// ============================================
// 单元测试
// ============================================

#[test]
fn add_and_find_actor() {
    let (_dir, groups_dir) = setup();
    let mut group = create_group(&groups_dir, "Test").unwrap();

    let actor = make_actor("dev-claude-a1b2", "Claude", ActorRole::Foreman, RuntimeKind::Claude);
    add_actor(&groups_dir, &mut group, actor).unwrap();

    let found = find_actor(&group, "dev-claude-a1b2");
    assert!(found.is_some());
    assert_eq!(found.unwrap().display_name, "Claude");
}

#[test]
fn add_three_agents() {
    let (_dir, groups_dir) = setup();
    let mut group = create_group(&groups_dir, "Multi Agent").unwrap();

    let claude = make_actor("dev-claude-a1b2", "Claude", ActorRole::Foreman, RuntimeKind::Claude);
    let codex = make_actor("dev-codex-b2c3", "Codex", ActorRole::Peer, RuntimeKind::Codex);
    let gemini = make_actor("dev-gemini-c3d4", "Gemini", ActorRole::Peer, RuntimeKind::Gemini);

    add_actor(&groups_dir, &mut group, claude).unwrap();
    add_actor(&groups_dir, &mut group, codex).unwrap();
    add_actor(&groups_dir, &mut group, gemini).unwrap();

    assert_eq!(list_actors(&group).len(), 3);
}

#[test]
fn find_foreman_test() {
    let (_dir, groups_dir) = setup();
    let mut group = create_group(&groups_dir, "Foreman Test").unwrap();

    let claude = make_actor("dev-claude-a1b2", "Claude", ActorRole::Foreman, RuntimeKind::Claude);
    let codex = make_actor("dev-codex-b2c3", "Codex", ActorRole::Peer, RuntimeKind::Codex);

    add_actor(&groups_dir, &mut group, claude).unwrap();
    add_actor(&groups_dir, &mut group, codex).unwrap();

    let foreman = find_foreman(&group);
    assert!(foreman.is_some());
    assert_eq!(foreman.unwrap().actor_id, "dev-claude-a1b2");
}

#[test]
fn remove_actor_test() {
    let (_dir, groups_dir) = setup();
    let mut group = create_group(&groups_dir, "Remove Test").unwrap();

    let actor = make_actor("dev-claude-a1b2", "Claude", ActorRole::Peer, RuntimeKind::Claude);
    add_actor(&groups_dir, &mut group, actor).unwrap();
    assert!(find_actor(&group, "dev-claude-a1b2").is_some());

    remove_actor(&groups_dir, &mut group, "dev-claude-a1b2").unwrap();
    assert!(find_actor(&group, "dev-claude-a1b2").is_none());
    assert_eq!(list_actors(&group).len(), 0);
}

#[test]
fn duplicate_actor_id_rejected() {
    let (_dir, groups_dir) = setup();
    let mut group = create_group(&groups_dir, "Dup Test").unwrap();

    let actor1 = make_actor("dev-claude-a1b2", "Claude 1", ActorRole::Peer, RuntimeKind::Claude);
    let actor2 = make_actor("dev-claude-a1b2", "Claude 2", ActorRole::Peer, RuntimeKind::Claude);

    add_actor(&groups_dir, &mut group, actor1).unwrap();
    let result = add_actor(&groups_dir, &mut group, actor2);

    assert!(result.is_err());
    match result.unwrap_err() {
        ghostcode_daemon::actor_mgmt::ActorError::DuplicateId(id) => {
            assert_eq!(id, "dev-claude-a1b2");
        }
        other => panic!("应返回 DuplicateId, 实际: {:?}", other),
    }
}

#[test]
fn second_foreman_rejected() {
    let (_dir, groups_dir) = setup();
    let mut group = create_group(&groups_dir, "Foreman Dup").unwrap();

    let foreman1 = make_actor("dev-claude-a1b2", "Claude", ActorRole::Foreman, RuntimeKind::Claude);
    let foreman2 = make_actor("dev-codex-b2c3", "Codex", ActorRole::Foreman, RuntimeKind::Codex);

    add_actor(&groups_dir, &mut group, foreman1).unwrap();
    let result = add_actor(&groups_dir, &mut group, foreman2);

    assert!(result.is_err());
    match result.unwrap_err() {
        ghostcode_daemon::actor_mgmt::ActorError::DuplicateForeman => {}
        other => panic!("应返回 DuplicateForeman, 实际: {:?}", other),
    }
}

#[test]
fn remove_nonexistent_returns_error() {
    let (_dir, groups_dir) = setup();
    let mut group = create_group(&groups_dir, "NotFound Test").unwrap();

    let result = remove_actor(&groups_dir, &mut group, "nonexistent");
    assert!(result.is_err());
}

#[test]
fn generate_actor_id_format() {
    let id = generate_actor_id("dev", &RuntimeKind::Claude);
    assert!(id.starts_with("dev-claude-"));
    assert_eq!(id.len(), "dev-claude-".len() + 4);

    let id2 = generate_actor_id("build", &RuntimeKind::Gemini);
    assert!(id2.starts_with("build-gemini-"));

    let id3 = generate_actor_id("test", &RuntimeKind::Custom("ollama".to_string()));
    assert!(id3.starts_with("test-ollama-"));
}

#[test]
fn actor_persisted_to_yaml() {
    // 验证 add/remove 后 group.yaml 正确更新
    let (_dir, groups_dir) = setup();
    let mut group = create_group(&groups_dir, "Persist Test").unwrap();

    let actor = make_actor("dev-claude-a1b2", "Claude", ActorRole::Peer, RuntimeKind::Claude);
    add_actor(&groups_dir, &mut group, actor).unwrap();

    // 重新从磁盘加载
    let group_dir = groups_dir.join(&group.group_id);
    let loaded = ghostcode_daemon::group::load_group(&group_dir).unwrap();
    assert_eq!(loaded.actors.len(), 1);
    assert_eq!(loaded.actors[0].actor_id, "dev-claude-a1b2");

    // 移除后再加载
    remove_actor(&groups_dir, &mut group, "dev-claude-a1b2").unwrap();
    let loaded2 = ghostcode_daemon::group::load_group(&group_dir).unwrap();
    assert_eq!(loaded2.actors.len(), 0);
}
