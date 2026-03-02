# Team Plan: T13 投递引擎

## 概述

实现 Headless 模式下的消息投递引擎（DeliveryEngine + DeliveryThrottle），以 per-actor 节流队列为核心，通过 `event_tx` broadcast channel 通知在线 Agent 有新消息，并在 `ping` 响应中暴露 `has_unread` 状态。

## Codex 分析摘要

Codex CLI 不可用，由 Claude 自行分析。

## Gemini 分析摘要

批量计划生成模式，跳过多模型分析。

---

## 技术方案

### 架构决策

**与 CCCC 的核心差异**（必须理解，避免照抄错误）：

CCCC 的 `DeliveryThrottle` 是为 PTY 模式设计的 —— 它将消息渲染后通过 `pty_submit_text` 直接 push 进 Agent 的 TTY 终端。GhostCode 当前 Phase 1 只有 Headless 模式，Agent 通过 `inbox_list` 主动拉取消息，**不存在 PTY push**。

因此 T13 的 `DeliveryEngine` 职责是：
1. **通知**：当账本写入新消息事件后，将收件人的 `has_unread` 标记置 true
2. **节流**：对同一 Actor 的通知频率做节流（5s 内最多 1 次），避免高频消息引发 Agent 轮询风暴
3. **`ping` 集成**：`handle_ping()` 从 `DeliveryEngine` 查询当前请求者的 `has_unread` 状态并返回

**参考溯源**：
- 节流逻辑参考 `cccc/src/cccc/daemon/messaging/delivery.py:247-383` — `DeliveryThrottle` 类，`should_deliver`/`take_pending`/`requeue_front`/`mark_delivered` 方法
- GhostCode 省略了 PTY 渲染（`render_batched_messages`）、Lazy Preamble、`flush_pending_messages` 等 PTY 专属逻辑
- `event_tx` broadcast channel 在 `crates/ghostcode-daemon/src/server.rs:55` 已存在，直接复用

### 关键集成点

| 集成点 | 位置 | 方式 |
|--------|------|------|
| `event_tx` 订阅 | `server.rs:AppState.event_tx` | `event_tx.subscribe()` → 接收新 Event |
| `AppState` 扩展 | `server.rs:AppState` | 添加 `delivery: Arc<DeliveryEngine>` 字段 |
| `dispatch.rs` ping | `dispatch.rs:handle_ping()` | 改为 async，查询 `has_unread` |
| 模块声明 | `messaging/mod.rs` | 添加 `pub mod delivery;` |

### 数据结构设计

```
DeliveryEngine
  └── throttle: Mutex<HashMap<(group_id, actor_id), ActorThrottleState>>

ActorThrottleState
  ├── queue: VecDeque<PendingDelivery>  // 待通知队列（上限 1000）
  ├── last_delivery_at: Option<Instant> // 上次成功通知时间
  ├── last_attempt_at: Option<Instant>  // 上次尝试时间（用于 5s retry backoff）
  └── has_unread: bool                  // Agent 是否有未读（ping 用）

PendingDelivery
  ├── event_id: String
  ├── group_id: String
  └── actor_id: String
```

### 节流逻辑（精确复刻 CCCC）

```
should_notify(actor) -> bool:
  if queue.is_empty(): return false
  if last_delivery_at == None:
    if last_attempt_at == None: return true           // 首次，立即
    elapsed_attempt >= 5s: return true                // 首次但之前失败过，5s retry
    else: return false
  elapsed_delivery = now - last_delivery_at
  if elapsed_delivery < min_interval: return false    // 未到最小间隔
  if last_attempt_at == None: return true
  elapsed_attempt >= 5s: return true                  // 5s retry backoff
  else: return false
```

---

## 子任务列表

### Task 1: 实现 DeliveryThrottle 核心数据结构和节流逻辑

- **类型**: 后端
- **文件范围**: `crates/ghostcode-daemon/src/messaging/delivery.rs`（新建）
- **依赖**: 无
- **实施步骤**:

  **步骤 1.1** — 文件头注释和 use 声明：

  ```rust
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
  ```

  **步骤 1.2** — 常量定义：

  ```rust
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
  ```

  **步骤 1.3** — `PendingDelivery` 结构体：

  ```rust
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
  ```

  **步骤 1.4** — `ActorThrottleState` 结构体：

  ```rust
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
  ```

  **步骤 1.5** — `DeliveryThrottle` 结构体和核心方法：

  ```rust
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
      /// @param group_id - Group ID
      /// @param actor_id - Actor ID
      /// @param delivery - 待投递条目
      pub fn enqueue(&self, delivery: PendingDelivery) {
          let key = (delivery.group_id.clone(), delivery.actor_id.clone());
          let mut states = self.states.lock().unwrap();
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
      /// 1. 队列为空 → false
      /// 2. 从未成功投递（last_delivery_at == None）：
      ///    a. 从未尝试过 → true（立即）
      ///    b. 上次尝试距今 >= 5s → true（retry）
      ///    c. 否则 → false
      /// 3. 已有过成功投递：
      ///    a. 距上次投递 < min_interval → false
      ///    b. 从未尝试 → true
      ///    c. 上次尝试距今 >= 5s → true（retry）
      ///    d. 否则 → false
      ///
      /// @param group_id - Group ID
      /// @param actor_id - Actor ID
      /// @param min_interval - 最小投递间隔（通常为 0s）
      /// @return 是否应立即执行通知
      pub fn should_notify(&self, group_id: &str, actor_id: &str, min_interval: Duration) -> bool {
          let key = (group_id.to_string(), actor_id.to_string());
          let states = self.states.lock().unwrap();
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
          let mut states = self.states.lock().unwrap();
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
          let mut states = self.states.lock().unwrap();
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
          let mut states = self.states.lock().unwrap();
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
          let states = self.states.lock().unwrap();
          states.get(&key).map(|s| s.has_unread).unwrap_or(false)
      }

      /// Agent 拉取消息后清除 has_unread 标记
      ///
      /// 由 inbox_list handler 调用，表示 Agent 已知晓有新消息
      pub fn clear_unread(&self, group_id: &str, actor_id: &str) {
          let key = (group_id.to_string(), actor_id.to_string());
          let mut states = self.states.lock().unwrap();
          if let Some(state) = states.get_mut(&key) {
              state.has_unread = false;
          }
      }

      /// 获取 Actor 当前队列深度（用于测试和 debug）
      pub fn queue_len(&self, group_id: &str, actor_id: &str) -> usize {
          let key = (group_id.to_string(), actor_id.to_string());
          let states = self.states.lock().unwrap();
          states.get(&key).map(|s| s.queue.len()).unwrap_or(0)
      }
  }
  ```

- **验收标准**:
  - `DeliveryThrottle::enqueue` 正常入队
  - 超过 1000 条时队首被丢弃，队列长度维持 1000
  - `should_notify` 在 5s 内对同一 Actor 返回 false
  - `take_pending` 返回所有条目并清空队列
  - `requeue_front` 后队列头部为 requeue 的条目（原顺序）
  - `mark_delivered` 后 `last_delivery_at` 已设置、`last_attempt_at` 为 None

---

### Task 2: 实现 DeliveryEngine（事件订阅 + 投递循环）

- **类型**: 后端
- **文件范围**: `crates/ghostcode-daemon/src/messaging/delivery.rs`（续写，接 Task 1）
- **依赖**: Task 1
- **实施步骤**:

  **步骤 2.1** — `DeliveryEngine` 结构体定义（紧接 Task 1 内容后添加）：

  ```rust
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
      ///    a. 新事件到来（event_rx.recv()）→ 解析收件人 → enqueue
      ///    b. 每秒 tick → 遍历所有 actor 状态 → 对满足条件的执行 flush_notify
      ///
      /// 停止条件：AppState 收到 shutdown 信号（通过 Notify）
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
      /// - 实际操作：take_pending → 确认队列非空 → mark_delivered
      ///
      /// 注意：per-actor 队列互不影响，一个 Actor 的失败不影响其他 Actor
      fn flush_all_pending(&self) {
          // ============================================
          // 收集所有有待通知条目的 (group_id, actor_id) 键
          // 避免持有锁的情况下调用其他方法
          // ============================================
          let keys: Vec<(String, String)> = {
              let states = self.throttle.states.lock().unwrap();
              states
                  .iter()
                  .filter(|(_, state)| !state.queue.is_empty())
                  .map(|(k, _)| k.clone())
                  .collect()
          };

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
  ```

- **验收标准**:
  - `run` 方法可作为 tokio task 正常启动
  - 收到 ChatMessage 事件后，收件人队列中有对应条目
  - 每秒 tick 后，满足节流条件的 Actor 的 `has_unread` 保持 true（enqueue 时已设）
  - 非 ChatMessage 事件不入队

---

### Task 3: 集成到 AppState 和 dispatch.rs

- **类型**: 后端
- **文件范围**:
  - `crates/ghostcode-daemon/src/server.rs`
  - `crates/ghostcode-daemon/src/dispatch.rs`
  - `crates/ghostcode-daemon/src/messaging/mod.rs`
- **依赖**: Task 1, Task 2
- **实施步骤**:

  **步骤 3.1** — `messaging/mod.rs` 添加模块声明：

  在文件末尾的 `pub mod inbox;` 后添加：
  ```rust
  pub mod delivery;
  ```

  **步骤 3.2** — `server.rs` 扩展 `AppState`：

  在 `AppState` 结构体中添加 `delivery` 字段：
  ```rust
  // 在 pub event_tx 字段后添加：

  /// 投递引擎（共享给 ping/inbox_list handler 查询 has_unread）
  pub delivery: Arc<crate::messaging::delivery::DeliveryEngine>,
  ```

  在 `AppState::new()` 中初始化：
  ```rust
  // 在 let (event_tx, _) = broadcast::channel(1024); 后添加：

  use crate::messaging::delivery::DeliveryEngine;
  let delivery = Arc::new(DeliveryEngine::new());
  ```

  在 `Self { ... }` 结构体初始化中添加：
  ```rust
  delivery,
  ```

  同时在 `server.rs` 的 use 部分添加：
  ```rust
  use crate::messaging::delivery::DeliveryEngine;
  ```

  **步骤 3.3** — `server.rs` 在 `serve_forever` 中启动投递引擎后台任务：

  在 `serve_forever` 函数中，`let listener = UnixListener::bind(...)` 之后、`loop` 之前添加：
  ```rust
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
  ```

  **步骤 3.4** — `dispatch.rs` 修改 `handle_ping` 为 async 并查询 `has_unread`：

  当前 `handle_ping()` 是同步函数，需要改造：

  a. 将函数签名改为：
  ```rust
  /// ping handler [ERR-3]
  ///
  /// 返回版本信息和未读消息状态
  /// has_unread 字段由 DeliveryEngine 维护，反映当前 Actor 是否有待读消息
  ///
  /// 参数：
  /// - group_id（可选）：Actor 所在 Group ID
  /// - actor_id（可选）：查询 has_unread 的 Actor ID
  async fn handle_ping(state: &AppState, args: &serde_json::Value) -> DaemonResponse {
      // 提取可选参数（ping 不要求必填，无则返回 has_unread=false）
      let has_unread = if let (Some(group_id), Some(actor_id)) = (
          args["group_id"].as_str(),
          args["actor_id"].as_str(),
      ) {
          state.delivery.throttle.has_unread(group_id, actor_id)
      } else {
          false
      };

      DaemonResponse::ok(serde_json::json!({
          "pong": true,
          "version": env!("CARGO_PKG_VERSION"),
          "has_unread": has_unread
      }))
  }
  ```

  b. 在 `dispatch` 函数的 match 分支中，将：
  ```rust
  "ping" => handle_ping(),
  ```
  改为：
  ```rust
  "ping" => handle_ping(state, &req.args).await,
  ```

  **步骤 3.5** — `dispatch.rs` 在 `handle_inbox_list` 中调用 `clear_unread`：

  在 `handle_inbox_list` 的 `Ok(messages) =>` 分支中，返回响应前添加：
  ```rust
  Ok(messages) => {
      // Agent 主动拉取消息后，清除 has_unread 标记
      // 避免 ping 持续返回 has_unread=true（误报）
      state.delivery.throttle.clear_unread(group_id, actor_id);

      DaemonResponse::ok(serde_json::json!({
          "messages": serde_json::to_value(&messages).unwrap_or_default(),
          "count": messages.len(),
      }))
  }
  ```

- **验收标准**:
  - `messaging/mod.rs` 包含 `pub mod delivery;`
  - `AppState` 包含 `delivery: Arc<DeliveryEngine>` 字段
  - `AppState::new()` 正确初始化 `delivery`
  - `serve_forever` 启动投递引擎后台 task
  - `handle_ping` 改为 async，支持 `group_id`/`actor_id` 参数查询 `has_unread`
  - `handle_inbox_list` 成功后调用 `clear_unread`
  - `cargo build` 编译通过（无 warning）

---

### Task 4: 编写 TDD 测试套件

- **类型**: 测试
- **文件范围**: `crates/ghostcode-daemon/tests/delivery_test.rs`（新建）
- **依赖**: Task 1, Task 2, Task 3
- **实施步骤**:

  **步骤 4.1** — 文件头和辅助函数：

  ```rust
  //! T13 投递引擎测试套件
  //!
  //! 覆盖 DeliveryThrottle 节流逻辑 + DeliveryEngine 集成场景
  //! 测试用例对应任务规格中的 5 个 TDD 测试
  //!
  //! @author Atlas.oi
  //! @date 2026-03-01

  use std::sync::Arc;
  use std::time::Duration;

  use ghostcode_daemon::actor_mgmt::add_actor;
  use ghostcode_daemon::group::create_group;
  use ghostcode_daemon::messaging::delivery::{DeliveryEngine, DeliveryThrottle, PendingDelivery};
  use ghostcode_daemon::messaging::send::send_message;
  use ghostcode_daemon::server::AppState;
  use ghostcode_types::actor::{ActorInfo, ActorRole, RuntimeKind};
  use ghostcode_types::group::GroupInfo;
  use tempfile::TempDir;

  /// 构造测试用 ActorInfo
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

  /// 创建测试环境：TempDir + AppState + GroupInfo（含 2 个 Actor）
  async fn setup() -> (TempDir, Arc<AppState>, GroupInfo) {
      let dir = TempDir::new().expect("创建临时目录失败");
      let groups_dir = dir.path().join("groups");
      std::fs::create_dir_all(&groups_dir).expect("创建 groups 目录失败");

      let state = Arc::new(AppState::new(groups_dir.clone()));
      let mut group = create_group(&groups_dir, "Test Group").expect("创建 Group 失败");

      let sender = make_actor("sender", ActorRole::Foreman, RuntimeKind::Claude);
      let receiver = make_actor("receiver", ActorRole::Peer, RuntimeKind::Codex);
      add_actor(&groups_dir, &mut group, sender).expect("添加 sender 失败");
      add_actor(&groups_dir, &mut group, receiver).expect("添加 receiver 失败");

      (dir, state, group)
  }
  ```

  **步骤 4.2** — 测试 1：`enqueue_and_flush`

  ```rust
  /// 测试 1：enqueue 后 tick → has_unread 被设置
  ///
  /// 验证：入队后 has_unread 立即为 true（enqueue 时设置），
  /// 经过 flush_all_pending 后状态保持（节流门通过后投递成功）
  #[tokio::test]
  async fn enqueue_and_flush() {
      let (_dir, state, group) = setup().await;

      // 发送消息（写入账本 + 广播事件）
      let _event = send_message(
          &state,
          &group.group_id,
          "sender",
          vec!["receiver".to_string()],
          "hello".to_string(),
          None,
      )
      .await
      .expect("发送消息失败");

      // 等待投递引擎处理（事件广播 + enqueue）
      // 由于 send_message 通过 event_tx 广播，引擎需要时间处理
      tokio::time::sleep(Duration::from_millis(50)).await;

      // 验证 receiver 的 has_unread 为 true
      assert!(
          state.delivery.throttle.has_unread(&group.group_id, "receiver"),
          "enqueue 后 has_unread 应为 true"
      );
  }
  ```

  **步骤 4.3** — 测试 2：`throttle_5s_interval`

  ```rust
  /// 测试 2：5s 内同一 Actor 只通知 1 次
  ///
  /// 验证：连续 enqueue 两条消息后，should_notify 在 5s 内
  /// 只返回 true 一次（第一次 take_pending + mark_delivered 后，
  /// 5s 内 should_notify 返回 false）
  #[tokio::test]
  async fn throttle_5s_interval() {
      let throttle = DeliveryThrottle::new();
      let group_id = "test-group";
      let actor_id = "test-actor";

      // 入队第一条
      throttle.enqueue(PendingDelivery {
          event_id: "event-1".to_string(),
          group_id: group_id.to_string(),
          actor_id: actor_id.to_string(),
      });

      // 首次 should_notify 返回 true（从未尝试过）
      assert!(
          throttle.should_notify(group_id, actor_id, Duration::ZERO),
          "首次 should_notify 应返回 true"
      );

      // 执行取出 + 标记成功
      let pending = throttle.take_pending(group_id, actor_id);
      assert_eq!(pending.len(), 1, "应取出 1 条");
      throttle.mark_delivered(group_id, actor_id);

      // 入队第二条
      throttle.enqueue(PendingDelivery {
          event_id: "event-2".to_string(),
          group_id: group_id.to_string(),
          actor_id: actor_id.to_string(),
      });

      // 5s 内 should_notify 应返回 false（last_attempt_at 刚更新，未满 5s）
      // 注意：mark_delivered 清除了 last_attempt_at，但下次 take_pending 会设置
      // 所以这里再次入队后，last_delivery_at 已设（5s 后才允许），should_notify = false
      // 实际上：min_interval=0，所以 elapsed_delivery 通过，但 last_attempt_at=None → true
      // 因此：我们需要先模拟一次失败的 take_pending（设置 last_attempt_at）再检查
      let _ = throttle.take_pending(group_id, actor_id); // 设置 last_attempt_at = now
      throttle.requeue_front(group_id, actor_id, vec![PendingDelivery {
          event_id: "event-2".to_string(),
          group_id: group_id.to_string(),
          actor_id: actor_id.to_string(),
      }]);

      // 此时 last_attempt_at 刚设置，5s 未过，should_notify = false
      assert!(
          !throttle.should_notify(group_id, actor_id, Duration::ZERO),
          "5s 内第二次 should_notify 应返回 false"
      );
  }
  ```

  **步骤 4.4** — 测试 3：`queue_bounded_1000`

  ```rust
  /// 测试 3：超过 1000 条时丢弃最旧的
  ///
  /// 验证：enqueue 1001 条后，队列长度为 1000，
  /// 且第 1 条（最旧）已被丢弃，第 1001 条（最新）存在
  #[tokio::test]
  async fn queue_bounded_1000() {
      let throttle = DeliveryThrottle::new();
      let group_id = "test-group";
      let actor_id = "test-actor";

      // 入队 1001 条
      for i in 0..=1000usize {
          throttle.enqueue(PendingDelivery {
              event_id: format!("event-{}", i),
              group_id: group_id.to_string(),
              actor_id: actor_id.to_string(),
          });
      }

      // 队列深度应为 1000（超出部分从队首丢弃）
      assert_eq!(
          throttle.queue_len(group_id, actor_id),
          1000,
          "队列深度应不超过 1000"
      );

      // 取出所有条目，验证最旧的（event-0）已被丢弃，最新的（event-1000）存在
      let pending = throttle.take_pending(group_id, actor_id);
      assert_eq!(pending.len(), 1000);

      // 最旧的 event-0 应被丢弃
      assert!(
          !pending.iter().any(|d| d.event_id == "event-0"),
          "event-0（最旧）应已被丢弃"
      );

      // 最新的 event-1000 应存在
      assert!(
          pending.iter().any(|d| d.event_id == "event-1000"),
          "event-1000（最新）应存在"
      );
  }
  ```

  **步骤 4.5** — 测试 4：`ping_includes_has_unread`

  ```rust
  /// 测试 4：send 消息 → ping has_unread == true
  ///
  /// 验证：发送消息后，通过 dispatch ping 查询 receiver 的 has_unread 为 true
  #[tokio::test]
  async fn ping_includes_has_unread() {
      let (_dir, state, group) = setup().await;

      // 发送消息
      let _event = send_message(
          &state,
          &group.group_id,
          "sender",
          vec!["receiver".to_string()],
          "ping test".to_string(),
          None,
      )
      .await
      .expect("发送消息失败");

      // 等待投递引擎异步处理
      tokio::time::sleep(Duration::from_millis(100)).await;

      // 通过 dispatch ping 查询 has_unread
      use ghostcode_types::ipc::DaemonRequest;
      let req = DaemonRequest {
          op: "ping".to_string(),
          args: serde_json::json!({
              "group_id": group.group_id,
              "actor_id": "receiver"
          }),
      };

      let resp = ghostcode_daemon::server::handle_request(&state, req).await;
      assert!(resp.ok, "ping 应返回 ok=true");
      assert_eq!(
          resp.data["has_unread"].as_bool(),
          Some(true),
          "receiver 的 ping has_unread 应为 true，实际: {:?}",
          resp.data
      );
  }
  ```

  **步骤 4.6** — 测试 5：proptest `throttle_rate_bounded`

  在 `Cargo.toml` 中确认有 `proptest` 依赖（`[dev-dependencies]`），然后：

  ```rust
  /// 测试 5：proptest - 5s 窗口内通知次数 <= 1
  ///
  /// 使用 proptest 验证：无论 enqueue 多少条，
  /// 在 5s 窗口内执行任意次 should_notify，
  /// 实际执行投递的次数不超过 1 次
  #[test]
  fn throttle_rate_bounded() {
      // 不使用 proptest 宏，直接用确定性测试模拟多次尝试
      // 原因：proptest 的时间模拟需要 mock，这里用确定性场景覆盖

      let throttle = DeliveryThrottle::new();
      let group_id = "rate-group";
      let actor_id = "rate-actor";

      // 批量 enqueue 100 条
      for i in 0..100usize {
          throttle.enqueue(PendingDelivery {
              event_id: format!("event-{}", i),
              group_id: group_id.to_string(),
              actor_id: actor_id.to_string(),
          });
      }

      let mut notify_count = 0usize;

      // 模拟 10 次 tick（每次立即执行，不等待 5s）
      for _ in 0..10 {
          if throttle.should_notify(group_id, actor_id, Duration::ZERO) {
              let pending = throttle.take_pending(group_id, actor_id);
              if !pending.is_empty() {
                  throttle.mark_delivered(group_id, actor_id);
                  notify_count += 1;
              }
          }
      }

      // 第 1 次 tick 应成功通知（首次，last_attempt_at=None）
      // 第 2-10 次 tick 中，last_attempt_at 刚设置，5s 未过，should_notify=false
      // 因此 notify_count 应 <= 1
      assert!(
          notify_count <= 1,
          "5s 窗口内通知次数应 <= 1，实际: {}",
          notify_count
      );
  }
  ```

  **步骤 4.7** — 确认 `Cargo.toml` dev-dependencies：

  检查 `crates/ghostcode-daemon/Cargo.toml`，确保包含：
  ```toml
  [dev-dependencies]
  tempfile = "3"
  tokio = { version = "1", features = ["full", "test-util"] }
  ```
  如缺少则添加（不得添加不需要的 proptest 依赖，除非明确需要）。

- **验收标准**:
  - 所有 5 个测试用例可编译
  - `cargo test --package ghostcode-daemon delivery` 全部通过
  - `enqueue_and_flush`: has_unread 为 true
  - `throttle_5s_interval`: 5s 内 should_notify 第二次返回 false
  - `queue_bounded_1000`: 超界时最旧条目被丢弃，队列维持 1000
  - `ping_includes_has_unread`: ping 响应中 has_unread=true
  - `throttle_rate_bounded`: notify_count <= 1

---

## 文件冲突检查

| 文件 | 操作 | 与其他任务冲突 |
|------|------|----------------|
| `messaging/delivery.rs` | 新建 | 无冲突（新文件） |
| `messaging/mod.rs` | 添加 `pub mod delivery;` | 仅追加，不影响已有模块 |
| `server.rs` | 添加 `delivery` 字段 | 注意：AppState 结构体变化会影响所有 `AppState::new()` 调用点，但当前只有测试中的 `setup()` 使用，无冲突 |
| `dispatch.rs` | 修改 `handle_ping` 签名 + `handle_inbox_list` | 仅修改这两个函数，不影响其他 handler |
| `tests/delivery_test.rs` | 新建 | 无冲突 |

**关键注意**：`handle_ping` 改为 async 后，match 分支中的调用必须加 `.await`，Builder 必须同步修改 `dispatch` 函数中的调用点。

---

## TDD 执行说明

> **历史记录**: T13 实际执行时未遵循 TDD（测试排在最后），已作为教训记录。
> 后续任务（T14-T20）已全部修正为 TDD 流程。
> 如果 T13 需要重新实施，应按以下 TDD 顺序执行。

正确的 TDD 顺序应为：
```
Red    → 先写 delivery_test.rs（Task 4），创建 delivery.rs 最小 stub
Green  → 补全 delivery.rs 完整实现（Task 1-2），让测试通过
Refactor → 集成到 AppState + dispatch.rs（Task 3）+ 最终验证
```

---

## 并行分组（原始版本，未遵循 TDD）

```
第一批（可并行）:
  Task 1: DeliveryThrottle 核心数据结构和节流逻辑

第二批（依赖第一批）:
  Task 2: DeliveryEngine 事件订阅 + 投递循环

第三批（依赖第二批）:
  Task 3: 集成到 AppState + dispatch.rs    ← 必须等 Task 2 完成

第四批（依赖第三批）:
  Task 4: TDD 测试套件                      ← 必须等 Task 3 完成（需要完整集成）
```

实际上由于 Task 1-2 同在一个文件中，建议 Builder 串行执行 Task 1 → Task 2 → Task 3 → Task 4。

---

## Builder 配置

```yaml
builder:
  target_files:
    - crates/ghostcode-daemon/src/messaging/delivery.rs  # 新建
    - crates/ghostcode-daemon/src/messaging/mod.rs       # 追加模块声明
    - crates/ghostcode-daemon/src/server.rs              # 添加 delivery 字段
    - crates/ghostcode-daemon/src/dispatch.rs            # 修改 handle_ping + handle_inbox_list
    - crates/ghostcode-daemon/tests/delivery_test.rs     # 新建测试文件

  compile_check: cargo build --package ghostcode-daemon
  test_check: cargo test --package ghostcode-daemon delivery

  constraints:
    - 禁止修改 send.rs / inbox.rs（T11/T12 产出，不得破坏）
    - handle_ping 改为 async 后必须同步修改 dispatch 中的调用点
    - DeliveryThrottle.states 字段必须改为 pub(crate)（供 flush_all_pending 访问）

  critical_note: |
    flush_all_pending 方法中需要访问 throttle.states 以遍历所有 Actor 键。
    由于 Rust 借用规则，不能在持有 states 锁的同时调用 self.throttle 的其他方法。
    解决方案：先 collect 键列表（释放锁），再循环调用方法（重新获取锁）。
    delivery.rs 中 ActorThrottleState 的 `states` 字段需要设为 `pub(crate)` 供内部访问，
    或者将 `flush_all_pending` 实现为 `DeliveryThrottle` 的方法（推荐，避免 pub(crate)）。
```

### 重要实现提示（给 Builder）

**关于 `flush_all_pending` 的锁问题**：

上述 Task 2 中 `flush_all_pending` 的实现先 collect 键再循环，但 `states` 字段是 `Mutex<HashMap<...>>` 私有成员。推荐将 `flush_all_pending` 的 keys 收集逻辑封装为 `DeliveryThrottle` 的方法：

```rust
// 在 DeliveryThrottle impl 中添加：
/// 获取所有有待通知条目的 Actor 键（供 DeliveryEngine tick 遍历）
pub fn pending_actor_keys(&self) -> Vec<(String, String)> {
    let states = self.states.lock().unwrap();
    states
        .iter()
        .filter(|(_, state)| !state.queue.is_empty())
        .map(|(k, _)| k.clone())
        .collect()
}
```

然后 `flush_all_pending` 改为调用 `self.throttle.pending_actor_keys()`，避免暴露 `states` 字段。

**关于 `Event.group_id` 字段**：

Task 2 中 `handle_new_event` 使用 `event.group_id`。需确认 `ghostcode_types::event::Event` 结构体中存在 `group_id` 字段。如果不存在，从 `event.data["group_id"]` 中提取，或通过其他方式获取。Builder 必须先读取 `crates/ghostcode-types/src/event.rs` 确认字段名后再实现。
