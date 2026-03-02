//! Actor 生命周期管理
//!
//! 实现 Actor 的 start/stop 操作及 Headless 状态管理：
//! - start_actor: 启动 Actor，创建 HeadlessSession，写入 ActorStart 事件
//! - stop_actor: 停止 Actor（幂等），移除 Session，写入 ActorStop 事件
//! - get_headless_status: 查询 Actor 运行状态快照
//! - set_headless_status: 更新 Actor 运行状态
//! - restore_running_actors: 从磁盘恢复所有 running=true 的 Actor
//! - spawn_heartbeat_monitor: 后台超时监控
//!
//! 参考: cccc/src/cccc/runners/headless.py - Headless session 状态机
//!
//! @author Atlas.oi
//! @date 2026-03-01

use std::sync::Arc;
use std::time::Duration;

use ghostcode_ledger::append_event;
use ghostcode_types::event::{Event, EventKind};

use crate::actor_mgmt::find_actor;
use crate::group::{list_groups, load_group};
use crate::runner::{HeadlessSession, HeadlessState, HeadlessStatus, LifecycleError};
use crate::server::AppState;

/// 生命周期操作结果类型别名
pub type Result<T> = std::result::Result<T, LifecycleError>;

/// 启动 Actor
///
/// 业务逻辑：
/// 1. 从磁盘加载 group.yaml，验证 group 和 actor 存在
/// 2. 检查 sessions 中无重复 session（幂等保护）
/// 3. 创建 HeadlessSession 并插入全局 sessions 表
/// 4. 更新 group.yaml 中 actor.running = true 并持久化
/// 5. 写入 ActorStart 事件到账本
///
/// @param state - 共享应用状态
/// @param group_id - Group ID
/// @param actor_id - Actor ID
/// @return 创建的 HeadlessState 快照
pub async fn start_actor(
    state: &AppState,
    group_id: &str,
    actor_id: &str,
) -> Result<HeadlessState> {
    // ============================================
    // 第一步：加载 group，验证 actor 存在
    // ============================================
    let group_dir = state.groups_dir.join(group_id);
    let mut group = load_group(&group_dir).map_err(|_| LifecycleError::GroupNotFound(group_id.to_string()))?;

    // 验证 actor 在 group 中存在
    if find_actor(&group, actor_id).is_none() {
        return Err(LifecycleError::ActorNotFound {
            group_id: group_id.to_string(),
            actor_id: actor_id.to_string(),
        });
    }

    // ============================================
    // 第二步：先完成磁盘持久化，再写入内存
    // 磁盘先于内存：若磁盘写入失败，内存状态不会被污染
    // ============================================

    // 更新 group.yaml 中 actor.running = true
    for actor in &mut group.actors {
        if actor.actor_id == actor_id {
            actor.running = true;
            break;
        }
    }
    let yaml = serde_yaml::to_string(&group)?;
    let yaml_path = group_dir.join("group.yaml");
    std::fs::write(&yaml_path, yaml)?;

    // 写入 ActorStart 事件到账本
    let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
    let lock_path = group_dir.join("state/ledger/ledger.lock");
    let event = Event::new(
        EventKind::ActorStart,
        group_id,
        "default",
        actor_id,
        serde_json::json!({ "actor_id": actor_id }),
    );
    append_event(&ledger_path, &lock_path, &event)?;

    // ============================================
    // 第三步：原子检查+插入（单写锁区间，消除竞态窗口）
    // 持久化已成功，此时才写入内存 sessions 表
    // ============================================
    let session = HeadlessSession::new(group_id, actor_id);
    let state_snapshot = session.to_state();

    {
        let mut sessions = state.sessions.write().await;
        let key = (group_id.to_string(), actor_id.to_string());
        if sessions.contains_key(&key) {
            return Err(LifecycleError::SessionAlreadyExists {
                group_id: group_id.to_string(),
                actor_id: actor_id.to_string(),
            });
        }
        sessions.insert(key, session);
    }

    Ok(state_snapshot)
}

/// 停止 Actor（幂等操作）
///
/// 业务逻辑：
/// 1. 从 sessions 移除（不存在也 OK，幂等）
/// 2. 从磁盘加载 group，更新 actor.running = false 并持久化
/// 3. 写入 ActorStop 事件到账本
///
/// @param state - 共享应用状态
/// @param group_id - Group ID
/// @param actor_id - Actor ID
pub async fn stop_actor(state: &AppState, group_id: &str, actor_id: &str) -> Result<()> {
    // ============================================
    // 第一步：从 sessions 移除（不存在也 OK，幂等）
    // ============================================
    {
        let mut sessions = state.sessions.write().await;
        sessions.remove(&(group_id.to_string(), actor_id.to_string()));
    }

    // ============================================
    // 第二步：加载 group，更新 actor.running = false
    // group 不存在时返回错误，actor 不存在时不更新（幂等）
    // ============================================
    let group_dir = state.groups_dir.join(group_id);
    let mut group = load_group(&group_dir).map_err(|_| LifecycleError::GroupNotFound(group_id.to_string()))?;

    for actor in &mut group.actors {
        if actor.actor_id == actor_id {
            actor.running = false;
            break;
        }
    }
    let yaml = serde_yaml::to_string(&group)?;
    let yaml_path = group_dir.join("group.yaml");
    std::fs::write(&yaml_path, yaml)?;

    // ============================================
    // 第三步：写入 ActorStop 事件到账本
    // ============================================
    let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
    let lock_path = group_dir.join("state/ledger/ledger.lock");
    let event = Event::new(
        EventKind::ActorStop,
        group_id,
        "default",
        actor_id,
        serde_json::json!({ "actor_id": actor_id }),
    );
    append_event(&ledger_path, &lock_path, &event)?;

    Ok(())
}

/// 查询 Headless Actor 状态
///
/// 从内存 sessions 表读取状态快照
/// Actor 未启动时返回 None
///
/// @param state - 共享应用状态
/// @param group_id - Group ID
/// @param actor_id - Actor ID
/// @return 状态快照，不存在时为 None
pub async fn get_headless_status(
    state: &AppState,
    group_id: &str,
    actor_id: &str,
) -> Option<HeadlessState> {
    let sessions = state.sessions.read().await;
    sessions
        .get(&(group_id.to_string(), actor_id.to_string()))
        .map(|s| s.to_state())
}

/// 更新 Headless Actor 状态
///
/// 更新内存 sessions 表中的状态，Actor 未启动时返回错误
///
/// @param state - 共享应用状态
/// @param group_id - Group ID
/// @param actor_id - Actor ID
/// @param status - 新状态
/// @param task_id - 关联任务 ID（可选）
/// @return 更新后的状态快照
pub async fn set_headless_status(
    state: &AppState,
    group_id: &str,
    actor_id: &str,
    status: HeadlessStatus,
    task_id: Option<String>,
) -> Result<HeadlessState> {
    // 获取写锁，更新状态，立即释放
    let mut sessions = state.sessions.write().await;
    let session = sessions
        .get_mut(&(group_id.to_string(), actor_id.to_string()))
        .ok_or_else(|| LifecycleError::SessionNotFound {
            group_id: group_id.to_string(),
            actor_id: actor_id.to_string(),
        })?;

    session.set_status(status, task_id);
    Ok(session.to_state())
}

/// 恢复所有 running=true 的 Actor
///
/// Daemon 重启后，从磁盘扫描所有 group，对 running=true 的 actor
/// 创建 HeadlessSession 并注入 sessions 表，恢复内存运行时状态
///
/// @param state - 共享应用状态
pub async fn restore_running_actors(state: &AppState) -> Result<()> {
    // ============================================
    // 第一步：列出所有 group
    // ============================================
    let groups = list_groups(&state.groups_dir)
        .map_err(|e| LifecycleError::Io(std::io::Error::other(e.to_string())))?;

    // ============================================
    // 第二步：遍历所有 actor，对 running=true 的创建 session
    // ============================================
    let mut sessions = state.sessions.write().await;
    for group in groups {
        for actor in &group.actors {
            if actor.running {
                let key = (group.group_id.clone(), actor.actor_id.clone());
                // 避免重复插入（理论上不会，但做防御性判断）
                if let std::collections::hash_map::Entry::Vacant(e) = sessions.entry(key) {
                    let session = HeadlessSession::new(&group.group_id, &actor.actor_id);
                    e.insert(session);
                }
            }
        }
    }

    Ok(())
}

/// 启动心跳超时监控（后台 Task）
///
/// 每 10 秒检查所有 session 是否超时
/// 超时的 session 标记状态为 Stopped，并写入 SystemNotify 事件到账本
///
/// @param state - 共享应用状态（Arc 持有，保证后台 task 生命周期）
/// @param timeout_secs - 超时阈值（秒）
pub fn spawn_heartbeat_monitor(state: Arc<AppState>, timeout_secs: u64) {
    tokio::spawn(async move {
        loop {
            // 每 10 秒检查一次
            tokio::time::sleep(Duration::from_secs(10)).await;

            // ============================================
            // 第一步：收集超时的 session key 列表
            // 读锁检查，释放后再写锁更新，避免嵌套锁
            // ============================================
            let timed_out_keys: Vec<(String, String)> = {
                let sessions = state.sessions.read().await;
                sessions
                    .iter()
                    .filter(|(_, s)| s.is_timed_out(timeout_secs))
                    .map(|(k, _)| k.clone())
                    .collect()
            };

            // ============================================
            // 第二步：逐个处理超时 session
            // 标记 Stopped + 写入 SystemNotify 事件
            // ============================================
            for (group_id, actor_id) in timed_out_keys {
                // 写锁移除 session（等效于 stop_actor 的内存清理）
                {
                    let mut sessions = state.sessions.write().await;
                    sessions.remove(&(group_id.clone(), actor_id.clone()));
                }

                // 更新 group.yaml 中 actor.running = false
                let group_dir = state.groups_dir.join(&group_id);
                if let Ok(mut group) = crate::group::load_group(&group_dir) {
                    for actor in &mut group.actors {
                        if actor.actor_id == actor_id {
                            actor.running = false;
                            break;
                        }
                    }
                    if let Ok(yaml) = serde_yaml::to_string(&group) {
                        let yaml_path = group_dir.join("group.yaml");
                        let _ = std::fs::write(&yaml_path, yaml);
                    }
                }

                // 写入 SystemNotify 事件（超时通知）
                let ledger_path = group_dir.join("state/ledger/ledger.jsonl");
                let lock_path = group_dir.join("state/ledger/ledger.lock");
                let event = Event::new(
                    EventKind::SystemNotify,
                    &group_id,
                    "default",
                    "daemon",
                    serde_json::json!({
                        "kind": "heartbeat_timeout",
                        "actor_id": actor_id,
                        "timeout_secs": timeout_secs,
                    }),
                );
                // 账本写入失败时记录但不中断监控循环
                if let Err(_e) = append_event(&ledger_path, &lock_path, &event) {
                    // 超时监控本身不应因账本写入失败而崩溃
                }
            }
        }
    });
}
