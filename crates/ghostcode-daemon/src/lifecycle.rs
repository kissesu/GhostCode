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
use crate::recovery::{on_actor_exit, RecoveryAction};
use crate::runner::{HeadlessSession, HeadlessState, HeadlessStatus, LifecycleError};
use crate::server::AppState;

/// 生命周期操作结果类型别名
pub type Result<T> = std::result::Result<T, LifecycleError>;

/// 启动 Actor
///
/// 业务逻辑：
/// 1. 获取写锁，保证整个 start 流程的原子性（C1 修复：消除 double-check 竞态窗口）
/// 2. 从磁盘加载 group.yaml，验证 group 存在
/// 3. 若 actor 未注册，自动注册到 group（D1 修复：SubagentStart Hook 传入的临时 ID 无需预注册）
/// 4. 检查 sessions 中无重复 session（幂等保护）
/// 5. 磁盘持久化：更新 group.yaml + 写入 ActorStart 事件
/// 6. 内存写入：创建 HeadlessSession 并插入 sessions 表
///
/// @param state - 共享应用状态
/// @param group_id - Group ID
/// @param actor_id - Actor ID
/// @param display_name - 人类可读显示名称（可选，来自 SubagentStart Hook）
/// @param agent_type - Agent 类型标识（可选，如 "feature-dev:code-reviewer"）
/// @return 创建的 HeadlessState 快照
pub async fn start_actor(
    state: &AppState,
    group_id: &str,
    actor_id: &str,
    display_name: Option<&str>,
    agent_type: Option<&str>,
) -> Result<HeadlessState> {
    // ============================================
    // 第一步：获取写锁（C1 修复）
    // 整个 start 流程在单一写锁内完成，消除读锁→写锁之间的竞态窗口
    // 两个并发 start 请求不会同时通过检查并各自写磁盘
    // ============================================
    let mut sessions = state.sessions.write().await;

    // ============================================
    // 第二步：幂等检查（在写锁内，无竞态）
    // ============================================
    let key = (group_id.to_string(), actor_id.to_string());
    if sessions.contains_key(&key) {
        return Err(LifecycleError::SessionAlreadyExists {
            group_id: group_id.to_string(),
            actor_id: actor_id.to_string(),
        });
    }

    // ============================================
    // 第三步：准备磁盘 I/O 所需的 owned 数据
    // 所有参数转为 owned 类型，供 spawn_blocking 闭包捕获
    // ============================================
    let group_dir = state.groups_dir.join(group_id);
    let actor_id_owned = actor_id.to_string();
    let group_id_owned = group_id.to_string();
    let display_name_owned = display_name.map(|s| s.to_string());
    let agent_type_owned = agent_type.map(|s| s.to_string());

    // ============================================
    // 第四步：磁盘 I/O 移入 spawn_blocking（C1-review 修复）
    // 同步文件操作（load_group、fs::write、append_event）在阻塞线程池执行，
    // 避免阻塞 tokio 工作线程。写锁跨 .await 点持有是 tokio::sync::RwLock
    // 的设计用途，保证原子性不受影响。
    //
    // 权衡说明：
    // - 写锁仍在 spawn_blocking 期间持有，其他 sessions 操作会等待
    // - 但 tokio 工作线程不再被阻塞，其他非 sessions 的异步任务可正常调度
    // - 对于本地开发工具（<10 并发 Actor），这是最佳平衡点
    // ============================================
    let group_dir_clone = group_dir.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        // 加载 group，自动注册未知 actor（D1 修复）
        let mut group = load_group(&group_dir_clone)
            .map_err(|_| LifecycleError::GroupNotFound(group_id_owned.clone()))?;

        if find_actor(&group, &actor_id_owned).is_none() {
            // 自动注册：SubagentStart Hook 传入的临时 Agent，无需预先 actor_add
            use ghostcode_types::actor::{ActorInfo, ActorRole, RuntimeKind};
            let new_actor = ActorInfo {
                actor_id: actor_id_owned.clone(),
                display_name: display_name_owned.as_deref().unwrap_or(&actor_id_owned).to_string(),
                role: ActorRole::Peer,
                runtime: RuntimeKind::Custom("claude-code".to_string()),
                running: false,
                pid: None,
            };
            group.actors.push(new_actor);
            tracing::info!(
                actor_id = actor_id_owned.as_str(),
                display_name = ?display_name_owned,
                "Actor 未预注册，已自动添加到 group"
            );
        }

        // 磁盘持久化（group.yaml + 账本事件）
        // 磁盘先于内存写入：若磁盘写入失败，内存状态不会被污染
        for actor in &mut group.actors {
            if actor.actor_id == actor_id_owned {
                actor.running = true;
                break;
            }
        }
        let yaml = serde_yaml::to_string(&group)?;
        let yaml_path = group_dir_clone.join("group.yaml");
        std::fs::write(&yaml_path, yaml)?;

        // 写入 ActorStart 事件到账本（携带可选的 display_name 和 agent_type）
        let ledger_path = group_dir_clone.join("state/ledger/ledger.jsonl");
        let lock_path = group_dir_clone.join("state/ledger/ledger.lock");
        let mut event_data = serde_json::json!({ "actor_id": actor_id_owned });
        if let Some(ref name) = display_name_owned {
            event_data["display_name"] = serde_json::Value::String(name.clone());
        }
        if let Some(ref atype) = agent_type_owned {
            event_data["agent_type"] = serde_json::Value::String(atype.clone());
        }
        let event = Event::new(
            EventKind::ActorStart,
            &group_id_owned,
            "default",
            &actor_id_owned,
            event_data,
        );
        append_event(&ledger_path, &lock_path, &event)?;

        Ok(())
    })
    .await
    .map_err(|e| LifecycleError::Io(std::io::Error::other(
        format!("spawn_blocking 任务 panic: {}", e)
    )))??;

    // ============================================
    // 第五步：内存写入（已在写锁内，无需二次获取）
    // ============================================
    let session = HeadlessSession::new(group_id, actor_id);
    let state_snapshot = session.to_state();
    sessions.insert(key, session);

    Ok(state_snapshot)
}

/// 停止 Actor（幂等操作）
///
/// 业务逻辑（C2 修复：磁盘先于内存）：
/// 1. 磁盘持久化：加载 group.yaml → 更新 running=false → 写回磁盘
/// 2. 写入 ActorStop 事件到账本
/// 3. 磁盘成功后，再从内存 sessions 移除
///
/// 这样当磁盘写入失败时，内存状态不会被提前修改，
/// Daemon 重启后不会错误恢复已停止的 Actor
///
/// W1-review 锁策略说明：
/// stop_actor 的磁盘操作在写锁之外执行，与 start_actor（锁内 spawn_blocking）策略不同。
/// 这是有意为之的设计差异：
/// - start 需要原子性：幂等检查→磁盘→内存 必须在同一锁内，防止并发 start 竞态
/// - stop 是幂等操作：磁盘写入失败时内存不受影响，重试 stop 即可恢复一致性
/// - stop 的写锁仅保护内存 sessions.remove，持有时间极短（微秒级）
/// - 这避免了 stop 操作在写锁内做磁盘 I/O 的不必要阻塞
///
/// @param state - 共享应用状态
/// @param group_id - Group ID
/// @param actor_id - Actor ID
pub async fn stop_actor(state: &AppState, group_id: &str, actor_id: &str) -> Result<()> {
    // ============================================
    // 第一步：磁盘持久化（先于内存操作）
    // 加载 group.yaml → 更新 actor.running = false → 写回
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
    // 第二步：写入 ActorStop 事件到账本
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

    // ============================================
    // 第三步：磁盘成功后，从内存 sessions 移除（不存在也 OK，幂等）
    // ============================================
    {
        let mut sessions = state.sessions.write().await;
        sessions.remove(&(group_id.to_string(), actor_id.to_string()));
    }

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
                // W6 修复：检查 remove 返回值，若已被其他路径移除则跳过后续操作
                {
                    let mut sessions = state.sessions.write().await;
                    if sessions.remove(&(group_id.clone(), actor_id.clone())).is_none() {
                        // 已被 stop_actor 或其他心跳周期移除，跳过重复处理
                        continue;
                    }
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

/// Actor 退出事件处理（供外部调用）
///
/// 当 Actor 进程异常退出时调用，返回受控的恢复动作。
/// 调用方根据 RecoveryAction 决定后续操作：
/// - Restart → 重新调用 start_actor
/// - MarkFailed → 更新 group.yaml actor.running = false，记录账本事件
/// - Cleaned → 无需进一步操作
///
/// 业务逻辑：
/// 1. 委托 recovery::on_actor_exit 决策恢复动作
/// 2. 将决策日志写入 tracing（供可观测性系统消费）
///
/// @param actor_id - Actor 标识符
/// @param exit_code - 退出码（进程正常退出时有值）
/// @param signal - 终止信号编号（被信号终止时有值）
/// @return RecoveryAction 恢复动作
pub fn handle_actor_exit(actor_id: &str, exit_code: Option<i32>, signal: Option<i32>) -> RecoveryAction {
    let action = on_actor_exit(actor_id, exit_code, signal);

    // 记录决策日志（供可观测性系统消费）
    match &action {
        RecoveryAction::Restart => {
            tracing::info!(
                actor_id = actor_id,
                exit_code = ?exit_code,
                signal = ?signal,
                "Actor 异常退出，恢复决策: Restart"
            );
        }
        RecoveryAction::MarkFailed { reason } => {
            tracing::warn!(
                actor_id = actor_id,
                exit_code = ?exit_code,
                signal = ?signal,
                reason = reason.as_str(),
                "Actor 异常退出，恢复决策: MarkFailed"
            );
        }
        RecoveryAction::Cleaned => {
            tracing::debug!(actor_id = actor_id, "Actor 退出，恢复决策: Cleaned");
        }
    }

    action
}
