//! Unix Socket 服务器
//!
//! Daemon 的网络层：监听 Unix Socket，接受连接，处理请求
//! 支持并发连接、请求超时、优雅关闭
//!
//! 参考: cccc/src/cccc/daemon/server.py:375-434
//!
//! @author Atlas.oi
//! @date 2026-03-01

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::BufReader;
use tokio::net::UnixListener;
use tokio::sync::{broadcast, Notify, RwLock};

use ghostcode_types::event::Event;
use ghostcode_types::ipc::{DaemonRequest, DaemonResponse};

use crate::hud::HudStateStore;
use crate::messaging::delivery::DeliveryEngine;
use crate::protocol::{self, ProtocolError};
use crate::routing::RoutingState;
use crate::runner::HeadlessSession;
use crate::verification::VerificationStateStore;

/// 单个请求处理超时：30 秒
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// 优雅关闭等待时间：2 秒
/// 等待在途请求完成，给新请求足够的缓冲时间
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

/// Daemon 配置
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// Unix Socket 文件路径
    pub socket_path: PathBuf,
}

/// 应用状态（在连接间共享）
///
/// 包含：
/// - shutdown: 优雅关闭信号
/// - groups_dir: groups 根目录路径，用于加载 group.yaml
/// - sessions: Headless Actor 运行时状态表（group_id + actor_id -> session）
/// - event_tx: 事件广播通道，用于内部事件发布/订阅
/// - routing: 路由状态管理器（Phase 2 新增，管理路由任务状态 + 代码主权守卫）
/// - verification: 验证状态存储（Phase 3 新增，Ralph 验证循环状态）
/// - hud_cache: HUD 状态缓存（Phase 3 新增，聚合状态快照供 Hook 查询）
pub struct AppState {
    /// 关闭信号
    shutdown: Notify,
    /// groups 根目录路径
    pub groups_dir: PathBuf,
    /// Headless Actor 会话表
    /// key: (group_id, actor_id)，RwLock 保证并发安全
    pub sessions: Arc<RwLock<HashMap<(String, String), HeadlessSession>>>,
    /// 事件广播发送端，容量 1024
    pub event_tx: broadcast::Sender<Event>,
    /// 投递引擎（共享给 ping/inbox_list handler 查询 has_unread）
    pub delivery: Arc<DeliveryEngine>,
    /// 路由状态管理器（Phase 2）
    /// 管理路由任务状态表 + SovereigntyGuard 代码主权检查
    pub routing: Arc<RoutingState>,
    /// 验证状态存储（Phase 3）
    /// Ralph 验证循环的状态数据
    /// 使用 Mutex 包装以支持 start_run/apply_event 的 &mut self 调用
    pub verification: Arc<std::sync::Mutex<VerificationStateStore>>,
    /// HUD 状态缓存（Phase 3）
    /// Hook 查询用的聚合状态快照
    pub hud_cache: Arc<HudStateStore>,
    /// Skill Learning 存储（Phase 4）
    /// C4 修复：以 group_id 为 key 隔离各 Group 的候选 Skill，防止数据泄露
    /// key: group_id, value: 该 group 的 SkillStore
    pub skill_store: std::sync::Mutex<HashMap<String, crate::skill_learning::SkillStore>>,
}

impl AppState {
    /// 创建新的应用状态
    ///
    /// @param groups_dir - groups 根目录路径
    pub fn new(groups_dir: PathBuf) -> Self {
        // 事件广播通道容量 1024，允许短时间的消费者滞后
        let (event_tx, _) = broadcast::channel(1024);
        let delivery = Arc::new(DeliveryEngine::new());
        Self {
            shutdown: Notify::new(),
            groups_dir,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            delivery,
            routing: Arc::new(RoutingState::new()),
            verification: Arc::new(std::sync::Mutex::new(VerificationStateStore::new())),
            hud_cache: Arc::new(HudStateStore::new()),
            // C4 修复：初始化为空 HashMap，按 group_id 动态创建 SkillStore
            skill_store: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// 触发关闭
    pub fn trigger_shutdown(&self) {
        self.shutdown.notify_waiters();
    }
}

impl Default for AppState {
    /// 默认使用测试用临时路径
    ///
    /// 仅用于测试环境，生产环境应传入真实 groups_dir
    fn default() -> Self {
        Self::new(PathBuf::from("/tmp/ghostcode-test/groups"))
    }
}

/// 请求处理器
///
/// 委托给 dispatch 模块进行路由
pub async fn handle_request(state: &AppState, req: DaemonRequest) -> DaemonResponse {
    crate::dispatch::dispatch(state, req).await
}

/// 处理单个连接
///
/// 从流中循环读取请求 → 处理 → 返回响应
/// 连接断开或出错时退出
///
/// @param stream - Unix Socket 连接
/// @param state - 共享应用状态
pub async fn handle_connection(
    stream: tokio::net::UnixStream,
    state: Arc<AppState>,
) -> std::result::Result<(), ProtocolError> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    loop {
        // 带超时读取请求
        let read_result = tokio::time::timeout(
            REQUEST_TIMEOUT,
            protocol::read_request(&mut reader),
        )
        .await;

        match read_result {
            // 超时
            Err(_) => {
                let resp = DaemonResponse::err("TIMEOUT", "request timeout");
                let _ = protocol::write_response(&mut write_half, &resp).await;
                break;
            }
            // 读取成功
            Ok(Ok(Some(req))) => {
                let resp = handle_request(&state, req).await;
                if let Err(_e) = protocol::write_response(&mut write_half, &resp).await {
                    break;
                }
            }
            // 连接关闭
            Ok(Ok(None)) => break,
            // 请求超大 [ERR-2]
            Ok(Err(ProtocolError::OversizedRequest(_, _))) => {
                break;
            }
            // JSON 解析错误 → 返回 error 响应，不断开连接
            Ok(Err(ProtocolError::Json(_))) => {
                let resp = DaemonResponse::err("INVALID_JSON", "malformed JSON request");
                if let Err(_e) = protocol::write_response(&mut write_half, &resp).await {
                    break;
                }
            }
            // IO 错误
            Ok(Err(ProtocolError::Io(_))) | Ok(Err(ProtocolError::ConnectionClosed)) => break,
        }
    }

    Ok(())
}

/// 启动 daemon 服务（主入口）
///
/// 监听 Unix Socket，接受连接并 spawn tokio task 处理
/// 收到关闭信号后停止接受新连接，等待在途请求完成
///
/// @param config - Daemon 配置
/// @param state - 共享应用状态
pub async fn serve_forever(
    config: DaemonConfig,
    state: Arc<AppState>,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    // socket 文件清理由 startup.rs 的 cleanup_stale_files 在获取单实例锁后统一负责
    // 此处不再重复删除，避免绕过锁保护误删正在使用的 socket
    let listener = UnixListener::bind(&config.socket_path)?;

    // ============================================
    // 启动投递引擎后台任务
    // 事件订阅 + 每秒 tick 通知在线 Agent
    // ============================================
    {
        let delivery = Arc::clone(&state.delivery);
        let state_for_delivery = Arc::clone(&state);
        tokio::spawn(async move {
            delivery.run(state_for_delivery).await;
        });
    }

    // 设置 socket 文件权限为 0o600（仅所有者可读写）
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&config.socket_path, perms)?;
    }

    loop {
        tokio::select! {
            // 等待新连接
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _addr)) => {
                        let state = Arc::clone(&state);
                        tokio::spawn(async move {
                            let _ = handle_connection(stream, state).await;
                        });
                    }
                    Err(e) => {
                        tracing::error!("accept 失败: {}", e);
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                }
            }
            // 等待关闭信号
            _ = state.shutdown.notified() => {
                // 停止接受新连接，等待在途请求完成
                tokio::time::sleep(SHUTDOWN_TIMEOUT).await;
                break;
            }
        }
    }

    // 清理 socket 文件
    let _ = std::fs::remove_file(&config.socket_path);

    Ok(())
}
