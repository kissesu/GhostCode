//! T10 Actor 生命周期测试套件
//!
//! 覆盖 start_actor/stop_actor/get_headless_status/set_headless_status/restore_running_actors/heartbeat
//!
//! @author Atlas.oi
//! @date 2026-03-01

use std::sync::Arc;

use ghostcode_daemon::actor_mgmt::add_actor;
use ghostcode_daemon::group::{create_group, load_group};
use ghostcode_daemon::lifecycle::{
    get_headless_status, restore_running_actors, set_headless_status, start_actor, stop_actor,
};
use ghostcode_daemon::runner::{HeadlessStatus, LifecycleError};
use ghostcode_daemon::server::AppState;
use ghostcode_types::actor::{ActorInfo, ActorRole, RuntimeKind};
use ghostcode_types::group::GroupInfo;
use tempfile::TempDir;

// ============================================
// 辅助函数
// ============================================

/// 构造测试用 ActorInfo
///
/// @param actor_id - Actor ID
/// @param role - 角色（Foreman 或 Peer）
/// @param runtime - 运行时类型
fn make_actor(actor_id: &str, role: ActorRole, runtime: RuntimeKind) -> ActorInfo {
    ActorInfo {
        actor_id: actor_id.to_string(),
        display_name: actor_id.to_string(),
        role,
        runtime,
        running: false,
        pid: None,
    }
}

/// 创建测试环境：临时目录 + AppState + Group（含 3 个 Actor）
///
/// 业务逻辑：
/// 1. 创建 TempDir，groups 目录在其下
/// 2. 创建 AppState::new(groups_dir)
/// 3. create_group → "Test Group"
/// 4. 添加 3 个 Actor：claude(Foreman)、codex(Peer)、gemini(Peer)
///    每个 actor running=false, pid=None
///
/// @return (TempDir 所有权保持 drop 安全, Arc<AppState>, GroupInfo)
async fn setup() -> (TempDir, Arc<AppState>, GroupInfo) {
    // ============================================
    // 第一步：创建临时目录和 groups 目录
    // ============================================
    let dir = TempDir::new().expect("创建临时目录失败");
    let groups_dir = dir.path().join("groups");
    std::fs::create_dir_all(&groups_dir).expect("创建 groups 目录失败");

    // ============================================
    // 第二步：创建 AppState
    // ============================================
    let state = Arc::new(AppState::new(groups_dir.clone()));

    // ============================================
    // 第三步：创建 Group
    // ============================================
    let mut group = create_group(&groups_dir, "Test Group").expect("创建 Group 失败");

    // ============================================
    // 第四步：添加 3 个 Actor
    // claude 为 Foreman，codex 和 gemini 为 Peer
    // ============================================
    let claude = make_actor("claude", ActorRole::Foreman, RuntimeKind::Claude);
    let codex = make_actor("codex", ActorRole::Peer, RuntimeKind::Codex);
    let gemini = make_actor("gemini", ActorRole::Peer, RuntimeKind::Gemini);

    add_actor(&groups_dir, &mut group, claude).expect("添加 claude actor 失败");
    add_actor(&groups_dir, &mut group, codex).expect("添加 codex actor 失败");
    add_actor(&groups_dir, &mut group, gemini).expect("添加 gemini actor 失败");

    (dir, state, group)
}

// ============================================
// 测试用例
// ============================================

/// 测试 1：start_actor 后 status 应为 Idle
///
/// 验证：start_actor 创建 session 并初始化 Idle 状态
#[tokio::test]
async fn start_actor_sets_idle() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    // start_actor 返回状态快照
    let snapshot = start_actor(&state, group_id, "claude")
        .await
        .expect("start_actor 应成功");

    assert_eq!(snapshot.actor_id, "claude");
    assert_eq!(snapshot.status, HeadlessStatus::Idle);

    // get_headless_status 也应返回 Idle
    let fetched = get_headless_status(&state, group_id, "claude").await;
    assert!(fetched.is_some(), "session 应存在");
    assert_eq!(fetched.unwrap().status, HeadlessStatus::Idle);
}

/// 测试 2：stop_actor 后 session 应被清理
///
/// 验证：stop_actor 从 sessions 表中移除 session
#[tokio::test]
async fn stop_actor_cleans_session() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    // 先启动
    start_actor(&state, group_id, "claude")
        .await
        .expect("start_actor 应成功");

    // 再停止
    stop_actor(&state, group_id, "claude")
        .await
        .expect("stop_actor 应成功");

    // session 应被移除
    let fetched = get_headless_status(&state, group_id, "claude").await;
    assert!(fetched.is_none(), "stop 后 session 应为 None");
}

/// 测试 3：stop_actor 幂等 - 多次调用不报错
///
/// 验证：stop_actor 在 session 不存在时仍然成功（幂等）
#[tokio::test]
async fn stop_idempotent() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    // 启动后停止
    start_actor(&state, group_id, "claude")
        .await
        .expect("start_actor 应成功");
    stop_actor(&state, group_id, "claude")
        .await
        .expect("第一次 stop_actor 应成功");

    // 第二次 stop 不应报错（幂等）
    stop_actor(&state, group_id, "claude")
        .await
        .expect("第二次 stop_actor 也应成功（幂等）");
}

/// 测试 4：状态转换 Idle -> Working -> Waiting -> Idle
///
/// 验证：set_headless_status 正确更新状态并可以在各状态间切换
#[tokio::test]
async fn status_transitions() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    // 启动后初始 Idle
    start_actor(&state, group_id, "claude")
        .await
        .expect("start_actor 应成功");

    // Idle -> Working
    let s = set_headless_status(
        &state,
        group_id,
        "claude",
        HeadlessStatus::Working,
        Some("task-001".to_string()),
    )
    .await
    .expect("切换到 Working 应成功");
    assert_eq!(s.status, HeadlessStatus::Working);
    assert_eq!(s.current_task_id.as_deref(), Some("task-001"));

    // Working -> Waiting
    let s = set_headless_status(&state, group_id, "claude", HeadlessStatus::Waiting, None)
        .await
        .expect("切换到 Waiting 应成功");
    assert_eq!(s.status, HeadlessStatus::Waiting);

    // Waiting -> Idle
    let s = set_headless_status(&state, group_id, "claude", HeadlessStatus::Idle, None)
        .await
        .expect("切换到 Idle 应成功");
    assert_eq!(s.status, HeadlessStatus::Idle);
}

/// 测试 5：restore_running_actors 从磁盘恢复 running=true 的 Actor
///
/// 验证：手动将所有 actor.running 设为 true 并写入 group.yaml，
///       新建 AppState 后调用 restore_running_actors，sessions 中应有 3 个 session
#[tokio::test]
async fn restore_running_actors_test() {
    let (dir, _old_state, group) = setup().await;
    let groups_dir = dir.path().join("groups");
    let group_id = &group.group_id;

    // ============================================
    // 第一步：手动修改 group.yaml，将所有 actor.running = true
    // ============================================
    let group_dir = groups_dir.join(group_id);
    let mut loaded_group = load_group(&group_dir).expect("加载 group 失败");

    for actor in &mut loaded_group.actors {
        actor.running = true;
    }
    assert_eq!(loaded_group.actors.len(), 3, "应有 3 个 actor");

    // 写回 group.yaml
    let yaml = serde_yaml::to_string(&loaded_group).expect("序列化失败");
    std::fs::write(group_dir.join("group.yaml"), yaml).expect("写入 group.yaml 失败");

    // ============================================
    // 第二步：创建全新 AppState 模拟 Daemon 重启
    // ============================================
    let new_state = Arc::new(AppState::new(groups_dir.clone()));

    // sessions 初始为空
    {
        let sessions = new_state.sessions.read().await;
        assert_eq!(sessions.len(), 0, "重启后 sessions 应为空");
    }

    // ============================================
    // 第三步：调用 restore_running_actors，恢复内存状态
    // ============================================
    restore_running_actors(&new_state)
        .await
        .expect("restore_running_actors 应成功");

    // sessions 应恢复 3 个
    {
        let sessions = new_state.sessions.read().await;
        assert_eq!(sessions.len(), 3, "恢复后 sessions 应有 3 个");
    }

    // 三个 actor 的状态应为 Idle
    for actor_id in &["claude", "codex", "gemini"] {
        let s = get_headless_status(&new_state, group_id, actor_id).await;
        assert!(
            s.is_some(),
            "actor {} 应有 session",
            actor_id
        );
        assert_eq!(
            s.unwrap().status,
            HeadlessStatus::Idle,
            "actor {} 恢复后应为 Idle",
            actor_id
        );
    }
}

/// 测试 6：心跳超时检测 - updated_at 超时后状态变为 Stopped
///
/// 验证：手动将 session.updated_at 设为 70 秒前，
///       调用心跳检查逻辑后 status 应变为 Stopped
#[tokio::test]
async fn heartbeat_timeout_detection() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    // 启动 actor
    start_actor(&state, group_id, "claude")
        .await
        .expect("start_actor 应成功");

    // ============================================
    // 第一步：手动将 session.updated_at 设为 70 秒前
    // 直接操作 sessions RwLock，模拟心跳超时场景
    // ============================================
    {
        let mut sessions = state.sessions.write().await;
        let key = (group_id.to_string(), "claude".to_string());
        let session = sessions.get_mut(&key).expect("session 应存在");

        // 设置 updated_at 为 70 秒前
        let past = chrono::Utc::now() - chrono::Duration::seconds(70);
        session.updated_at =
            past.to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
    }

    // ============================================
    // 第二步：验证 is_timed_out 检测正确
    // ============================================
    {
        let sessions = state.sessions.read().await;
        let key = (group_id.to_string(), "claude".to_string());
        let session = sessions.get(&key).expect("session 应存在");
        // 超时阈值 60 秒，updated_at 已是 70 秒前，应超时
        assert!(
            session.is_timed_out(60),
            "70 秒后应超时（阈值 60 秒）"
        );
    }

    // ============================================
    // 第三步：模拟心跳监控逻辑：收集超时 key，标记 Stopped
    // 复用与 spawn_heartbeat_monitor 相同的逻辑进行直接测试，
    // 避免等待 10 秒周期
    // ============================================
    let timeout_secs: u64 = 60;
    let timed_out_keys: Vec<(String, String)> = {
        let sessions = state.sessions.read().await;
        sessions
            .iter()
            .filter(|(_, s)| s.is_timed_out(timeout_secs))
            .map(|(k, _)| k.clone())
            .collect()
    };

    // 应有 1 个超时 session
    assert_eq!(timed_out_keys.len(), 1, "应有 1 个超时 session");

    // 写锁标记 Stopped
    {
        let mut sessions = state.sessions.write().await;
        for (gid, aid) in &timed_out_keys {
            if let Some(session) = sessions.get_mut(&(gid.clone(), aid.clone())) {
                session.set_status(HeadlessStatus::Stopped, None);
            }
        }
    }

    // 验证状态已变为 Stopped
    let s = get_headless_status(&state, group_id, "claude")
        .await
        .expect("session 应存在");
    assert_eq!(
        s.status,
        HeadlessStatus::Stopped,
        "超时后状态应为 Stopped"
    );
}

/// 测试 7：start_actor 使用不存在的 actor_id 应返回 ActorNotFound
///
/// 验证：actor_id 不在 group 中时 start_actor 返回正确错误类型
#[tokio::test]
async fn start_nonexistent_actor_fails() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    let result = start_actor(&state, group_id, "nonexistent-actor").await;

    assert!(result.is_err(), "不存在的 actor 应返回错误");
    match result.unwrap_err() {
        LifecycleError::ActorNotFound { actor_id, .. } => {
            assert_eq!(actor_id, "nonexistent-actor");
        }
        other => panic!("期望 ActorNotFound，实际: {:?}", other),
    }
}

/// 测试 8：重复 start 同一 actor 应返回 SessionAlreadyExists
///
/// 验证：同一 actor 已有 session 时，再次 start 应返回幂等保护错误
#[tokio::test]
async fn start_duplicate_session_fails() {
    let (_dir, state, group) = setup().await;
    let group_id = &group.group_id;

    // 第一次 start 应成功
    start_actor(&state, group_id, "claude")
        .await
        .expect("第一次 start_actor 应成功");

    // 第二次 start 同一 actor 应失败
    let result = start_actor(&state, group_id, "claude").await;
    assert!(result.is_err(), "重复 start 应返回错误");
    match result.unwrap_err() {
        LifecycleError::SessionAlreadyExists { actor_id, .. } => {
            assert_eq!(actor_id, "claude");
        }
        other => panic!("期望 SessionAlreadyExists，实际: {:?}", other),
    }
}
