//! 消息投递引擎
//!
//! Headless 模式下的消息投递节流引擎
//! 通过 per-actor 队列 + 节流控制，确保 Agent 及时感知新消息
//! 不直接 push 消息，而是维护 has_unread 标记并通过 event_tx 广播通知
//!
//! 参考: cccc/src/cccc/daemon/messaging/delivery.py:247-461
//! GhostCode 差异: 省略 PTY push / Lazy Preamble / 消息渲染，只保留节流通知核心
//!
//! @author Atlas.oi
//! @date 2026-03-01

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ghostcode_types::event::Event;

use crate::server::AppState;

// ============================================
// 常量定义
// ============================================

/// 投递 retry backoff 间隔：5 秒
/// 同一 Actor 在首次投递失败后，或后续投递后，至少等待此时间再重试
/// 参考: cccc/delivery.py:50 DEFAULT_DELIVERY_RETRY_INTERVAL_SECONDS = 5
const DELIVERY_RETRY_INTERVAL: Duration = Duration::from_secs(5);

/// 默认最小投递间隔：0 秒（不限制）
/// 参考: cccc/delivery.py:49 DEFAULT_DELIVERY_MIN_INTERVAL_SECONDS = 0
const DEFAULT_MIN_INTERVAL: Duration = Duration::ZERO;

/// 投递循环 tick 频率：每秒 1 次
const TICK_INTERVAL: Duration = Duration::from_secs(1);

/// per-actor 队列最大深度：超出时丢弃最旧的
const MAX_QUEUE_DEPTH: usize = 1000;

// ============================================
// 数据结构
// ============================================

/// 待投递条目
///
/// 代表一条已写入账本、待通知 Agent 的消息引用
/// 仅存储必要的索引信息，不存储消息内容（Agent 通过 inbox_list 拉取）
#[derive(Debug, Clone)]
pub struct PendingDelivery {
    /// 消息事件 ID（账本中的唯一标识）
    pub event_id: String,
    /// 所属 Group ID
    pub group_id: String,
    /// 目标 Actor ID
    pub actor_id: String,
}

/// 单个 Actor 的投递节流状态
///
/// 参考: cccc/delivery.py:238-244 ActorDeliveryState
/// GhostCode 简化：去掉 delivered_chat_count（PTY 专用）
#[derive(Debug)]
struct ActorThrottleState {
    /// 待通知队列，最大 MAX_QUEUE_DEPTH 条
    queue: VecDeque<PendingDelivery>,
    /// 上次成功通知时间（None = 从未成功投递过）
    last_delivery_at: Option<Instant>,
    /// 上次尝试时间（包含失败尝试，用于 5s retry backoff）
    last_attempt_at: Option<Instant>,
    /// 该 Actor 是否有未读消息（供 ping 查询）
    has_unread: bool,
}

impl Default for ActorThrottleState {
    fn default() -> Self {
        Self {
            queue: VecDeque::new(),
            last_delivery_at: None,
            last_attempt_at: None,
            has_unread: false,
        }
    }
}

// ============================================
// DeliveryThrottle 节流器
// ============================================

/// 消息投递节流器
///
/// 维护所有 Group 下所有 Actor 的投递状态
/// 线程安全：通过 Mutex<HashMap> 实现
/// 参考: cccc/delivery.py:247-461 DeliveryThrottle
pub struct DeliveryThrottle {
    /// 状态表：(group_id, actor_id) -> 投递状态
    states: Mutex<HashMap<(String, String), ActorThrottleState>>,
}

impl DeliveryThrottle {
    /// 创建新的节流器实例
    pub fn new() -> Self {
        Self {
            states: Mutex::new(HashMap::new()),
        }
    }

    /// 将消息加入指定 Actor 的待通知队列
    ///
    /// 超过 MAX_QUEUE_DEPTH 时，丢弃队列中最旧的一条（从队首弹出）
    ///
    /// @param delivery - 待投递条目
    pub fn enqueue(&self, delivery: PendingDelivery) {
        let key = (delivery.group_id.clone(), delivery.actor_id.clone());
        let mut states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        let state = states.entry(key).or_default();

        // 队列满时丢弃最旧的（队首）
        if state.queue.len() >= MAX_QUEUE_DEPTH {
            state.queue.pop_front();
            tracing::warn!(
                "投递队列已满（1000），丢弃最旧消息: group={} actor={}",
                delivery.group_id,
                delivery.actor_id
            );
        }
        state.queue.push_back(delivery);
        // 有新消息入队，立即标记 has_unread
        state.has_unread = true;
    }

    /// 判断当前是否应该对指定 Actor 执行通知
    ///
    /// 节流规则（精确复刻 cccc/delivery.py:304-326 should_deliver）：
    /// 1. 队列为空 -> false
    /// 2. 从未成功投递（last_delivery_at == None）：
    ///    a. 从未尝试过 -> true（立即）
    ///    b. 上次尝试距今 >= 5s -> true（retry）
    ///    c. 否则 -> false
    /// 3. 已有过成功投递：
    ///    a. 距上次投递 < min_interval -> false
    ///    b. 从未尝试 -> true
    ///    c. 上次尝试距今 >= 5s -> true（retry）
    ///    d. 否则 -> false
    ///
    /// @param group_id - Group ID
    /// @param actor_id - Actor ID
    /// @param min_interval - 最小投递间隔（通常为 0s）
    /// @return 是否应立即执行通知
    pub fn should_notify(&self, group_id: &str, actor_id: &str, min_interval: Duration) -> bool {
        let key = (group_id.to_string(), actor_id.to_string());
        let states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        let Some(state) = states.get(&key) else {
            return false;
        };

        if state.queue.is_empty() {
            return false;
        }

        let now = Instant::now();

        if state.last_delivery_at.is_none() {
            // 从未成功投递过
            let Some(last_attempt) = state.last_attempt_at else {
                return true; // 从未尝试过，立即执行
            };
            return now.duration_since(last_attempt) >= DELIVERY_RETRY_INTERVAL;
        }

        // 已有过成功投递
        let last_delivery = state.last_delivery_at.unwrap();
        if now.duration_since(last_delivery) < min_interval {
            return false;
        }

        let Some(last_attempt) = state.last_attempt_at else {
            return true;
        };
        now.duration_since(last_attempt) >= DELIVERY_RETRY_INTERVAL
    }

    /// 取出并清空 Actor 的待通知队列（同时标记 last_attempt_at）
    ///
    /// 参考: cccc/delivery.py:328-336 take_pending
    ///
    /// @return 取出的待投递列表
    pub fn take_pending(&self, group_id: &str, actor_id: &str) -> Vec<PendingDelivery> {
        let key = (group_id.to_string(), actor_id.to_string());
        let mut states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        let state = states.entry(key).or_default();
        let items: Vec<PendingDelivery> = state.queue.drain(..).collect();
        state.last_attempt_at = Some(Instant::now());
        items
    }

    /// 将投递条目放回队列头部（用于失败重试，保持消息顺序）
    ///
    /// 参考: cccc/delivery.py:338-345 requeue_front
    ///
    /// @param deliveries - 要放回的条目列表（顺序保持不变）
    pub fn requeue_front(&self, group_id: &str, actor_id: &str, deliveries: Vec<PendingDelivery>) {
        if deliveries.is_empty() {
            return;
        }
        let key = (group_id.to_string(), actor_id.to_string());
        let mut states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        let state = states.entry(key).or_default();
        // 将 deliveries 插入队列头部（逆序 push_front 以保持原顺序）
        for d in deliveries.into_iter().rev() {
            state.queue.push_front(d);
        }
    }

    /// 标记成功投递（重置 last_delivery_at，清除 last_attempt_at）
    ///
    /// 参考: cccc/delivery.py:347-353 mark_delivered
    /// 成功后清除 last_attempt_at，避免 retry backoff 误触发
    pub fn mark_delivered(&self, group_id: &str, actor_id: &str) {
        let key = (group_id.to_string(), actor_id.to_string());
        let mut states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        let state = states.entry(key).or_default();
        state.last_delivery_at = Some(Instant::now());
        state.last_attempt_at = None; // 成功投递不应触发 retry backoff
    }

    /// 查询 Actor 是否有未读消息
    ///
    /// 供 ping handler 调用，返回 has_unread 状态
    ///
    /// @return true 表示有未读消息等待 Agent 拉取
    pub fn has_unread(&self, group_id: &str, actor_id: &str) -> bool {
        let key = (group_id.to_string(), actor_id.to_string());
        let states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        states.get(&key).map(|s| s.has_unread).unwrap_or(false)
    }

    /// Agent 拉取消息后清除 has_unread 标记
    ///
    /// 由 inbox_list handler 调用，表示 Agent 已知晓有新消息
    pub fn clear_unread(&self, group_id: &str, actor_id: &str) {
        let key = (group_id.to_string(), actor_id.to_string());
        let mut states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(state) = states.get_mut(&key) {
            state.has_unread = false;
        }
    }

    /// 获取 Actor 当前队列深度（用于测试和 debug）
    pub fn queue_len(&self, group_id: &str, actor_id: &str) -> usize {
        let key = (group_id.to_string(), actor_id.to_string());
        let states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        states.get(&key).map(|s| s.queue.len()).unwrap_or(0)
    }

    /// 获取所有有待通知条目的 Actor 键（供 DeliveryEngine tick 遍历）
    ///
    /// 先收集键列表再释放锁，避免在持有锁的情况下调用其他方法
    pub fn pending_actor_keys(&self) -> Vec<(String, String)> {
        let states = self.states.lock().unwrap_or_else(|e| e.into_inner());
        states
            .iter()
            .filter(|(_, state)| !state.queue.is_empty())
            .map(|(k, _)| k.clone())
            .collect()
    }
}

// ============================================
// DeliveryEngine 投递引擎
// ============================================

/// 投递引擎
///
/// 负责：
/// 1. 订阅 event_tx broadcast channel，监听新消息事件
/// 2. 将消息收件人的待通知条目加入节流队列
/// 3. 每秒 tick 一次，对满足节流条件的 Actor 执行通知（设置 has_unread）
///
/// Headless 模式说明：
/// 不直接投递消息内容，仅维护 has_unread 标记
/// Agent 通过轮询 ping（检查 has_unread）后主动调用 inbox_list 拉取
pub struct DeliveryEngine {
    /// 节流器（共享给 ping/inbox_list handler）
    pub throttle: Arc<DeliveryThrottle>,
}

impl DeliveryEngine {
    /// 创建新的投递引擎实例
    pub fn new() -> Self {
        Self {
            throttle: Arc::new(DeliveryThrottle::new()),
        }
    }

    /// 将事件的收件人加入投递队列
    ///
    /// 由消息写入后（send/reply）的回调路径调用
    /// 从 Event.data["to"] 提取收件人列表，为每个收件人入队
    ///
    /// @param group_id - Group ID
    /// @param event - 已写入账本的事件
    /// @param recipients - 收件人 actor_id 列表
    pub fn enqueue(&self, group_id: &str, event: &Event, recipients: &[String]) {
        for actor_id in recipients {
            let delivery = PendingDelivery {
                event_id: event.id.clone(),
                group_id: group_id.to_string(),
                actor_id: actor_id.clone(),
            };
            self.throttle.enqueue(delivery);
        }
    }

    /// 投递引擎主循环
    ///
    /// 业务逻辑：
    /// 1. 订阅 event_tx broadcast channel
    /// 2. 使用 tokio::select! 同时监听：
    ///    a. 新事件到来（event_rx.recv()）-> 解析收件人 -> enqueue
    ///    b. 每秒 tick -> 遍历所有 actor 状态 -> 对满足条件的执行 flush_notify
    ///
    /// 停止条件：broadcast channel 关闭（Daemon 关闭时 event_tx 被 drop）
    ///
    /// @param state - 共享应用状态（含 event_tx、shutdown）
    pub async fn run(self: Arc<Self>, state: Arc<AppState>) {
        let mut event_rx = state.event_tx.subscribe();
        let mut tick = tokio::time::interval(TICK_INTERVAL);

        loop {
            tokio::select! {
                // ============================================
                // 分支 1：新事件到来
                // 从 event_tx broadcast channel 接收新写入的 Event
                // 解析 data["to"] 字段提取收件人，入队
                // ============================================
                result = event_rx.recv() => {
                    match result {
                        Ok(event) => {
                            self.handle_new_event(&event);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            // 广播通道滞后，部分事件被丢弃，仅记录警告
                            // 不做降级，Agent 通过 inbox_list 仍能拉取到消息
                            tracing::warn!("投递引擎广播通道滞后，丢失 {} 个事件通知", n);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            // 通道关闭，Daemon 正在关闭
                            tracing::info!("投递引擎广播通道已关闭，停止事件监听");
                            break;
                        }
                    }
                }

                // ============================================
                // 分支 2：每秒 tick
                // 遍历所有 Actor 状态，对满足节流条件的执行通知
                // ============================================
                _ = tick.tick() => {
                    self.flush_all_pending();
                }
            }
        }
    }

    /// 处理新到来的事件，将收件人入队
    ///
    /// 只处理 ChatMessage 类型事件（其他事件类型不需要投递通知）
    /// 从 event.data["to"] 提取收件人列表
    ///
    /// @param event - 从 broadcast channel 接收到的事件
    fn handle_new_event(&self, event: &Event) {
        use ghostcode_types::event::EventKind;

        // 只处理 ChatMessage 事件
        if event.kind != EventKind::ChatMessage {
            return;
        }

        // 提取 group_id（event 中已有）
        let group_id = &event.group_id;

        // 从 data["to"] 提取收件人列表
        let recipients: Vec<String> = event
            .data
            .get("to")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        if recipients.is_empty() {
            tracing::debug!("事件 {} 无收件人，跳过投递入队", event.id);
            return;
        }

        self.enqueue(group_id, event, &recipients);
        tracing::debug!(
            "事件 {} 已入队，收件人: {:?}",
            event.id,
            recipients
        );
    }

    /// 遍历所有待通知 Actor，对满足节流条件的执行通知
    ///
    /// 通知的含义（Headless 模式）：
    /// - has_unread 标记在 enqueue 时已设置为 true
    /// - flush_all_pending 的作用是确认节流窗口已过，可以允许 Agent 收到 ping 通知
    /// - 实际操作：take_pending -> 确认队列非空 -> mark_delivered
    ///
    /// 注意：per-actor 队列互不影响，一个 Actor 的失败不影响其他 Actor
    fn flush_all_pending(&self) {
        // ============================================
        // 收集所有有待通知条目的 (group_id, actor_id) 键
        // 通过 pending_actor_keys() 方法避免在持有锁的情况下调用其他方法
        // ============================================
        let keys = self.throttle.pending_actor_keys();

        for (group_id, actor_id) in keys {
            if self.throttle.should_notify(&group_id, &actor_id, DEFAULT_MIN_INTERVAL) {
                // 取出待通知条目（清空队列，更新 last_attempt_at）
                let pending = self.throttle.take_pending(&group_id, &actor_id);
                if pending.is_empty() {
                    continue;
                }

                // Headless 模式：通知成功 = 节流门已开，Agent 下次 ping 可收到 has_unread=true
                // has_unread 在 enqueue 时已设置，这里只需标记投递成功（更新时间戳）
                self.throttle.mark_delivered(&group_id, &actor_id);

                tracing::debug!(
                    "已通知 Actor {}/{} 有 {} 条新消息",
                    group_id,
                    actor_id,
                    pending.len()
                );
            }
        }
    }
}
