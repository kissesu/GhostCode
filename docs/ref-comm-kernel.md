# GhostCode 实现参考：通信内核 + Agent 管理

> 源码来源：`/Users/oi/CodeCoding/Code/github/claude-plugin/cccc/`
> 验证时间：2026-02-28
> 用途：GhostCode 用 Rust 重新实现通信内核的参考

---

## 一、Daemon 启动流程

### 启动序列

```
CLI "start" → call_daemon({"op":"ping"}) 检查是否运行
  → 未运行: subprocess.Popen([python, "-m", "cccc.daemon_main", "run"], start_new_session=True)
  → "run" 命令调用 serve_forever(paths)
```

文件：`src/cccc/daemon_main.py` + `src/cccc/daemon/server.py:747-759`

### 单实例锁

文件：`src/cccc/util/file_lock.py`

- 锁文件：`~/.cccc/daemon/ccccd.lock`
- POSIX：`fcntl.flock(fd, LOCK_EX | LOCK_NB)` — 进程级排他锁，进程死亡自动释放
- Windows：`msvcrt.locking(fd, LK_NBLCK, 1)`
- `blocking=False` — 第二个 daemon 立即失败

**Rust 对应**：`fs2::FileExt::try_lock_exclusive()`

### IPC 通道

文件：`src/cccc/daemon/server.py:375-434`

```
POSIX: AF_UNIX socket → ~/.cccc/daemon/ccccd.sock
Windows: AF_INET TCP → 127.0.0.1:9765
```

端点描述符写入：`~/.cccc/daemon/ccccd.addr.json`
```json
{ "v":1, "transport":"unix", "path":"...", "pid":12345, "version":"...", "ts":"..." }
```

**协议**：单行 JSON + `\n`，每个连接独立

**Rust 对应**：`tokio::net::UnixListener`

### 关键目录结构

```
~/.cccc/
  daemon/
    ccccd.lock           -- 单实例锁
    ccccd.sock           -- Unix Socket
    ccccd.addr.json      -- 端点描述符
    ccccd.pid            -- PID 文件
    ccccd.log            -- 日志
  groups/<group_id>/
    group.yaml           -- Group 配置
    state/
      ledger/
        ledger.jsonl     -- 事件账本
        ledger.lock      -- 账本写锁
        blobs/           -- 大消息溢出
      read_cursors.json  -- 已读游标
```

---

## 二、Append-only 事件账本

### 事件格式

文件：`src/cccc/kernel/ledger.py` + `src/cccc/contracts/v1/event.py:189-199`

每行一个 JSON（NDJSON/JSONL）：

```json
{
  "v": 1,
  "id": "32位十六进制uuid",
  "ts": "2026-02-28T12:00:00.000000+00:00",
  "kind": "chat.message",
  "group_id": "...",
  "scope_key": "",
  "by": "actor_id或user",
  "data": { ... }
}
```

### 完整 EventKind 列表（23 种）

```
group: create/update/attach/detach_scope/set_active_scope/start/stop/set_state/settings_update/automation_update
actor: add/update/set_role/start/stop/restart/remove
context: sync
chat: message/ack/read/reaction
system: notify/notify_ack
```

### 写入原子性

文件：`src/cccc/kernel/ledger.py:60-103`

双重保护：
1. **文件锁**（`ledger.lock`）：`acquire_lockfile(lock, blocking=True)`
2. **大消息溢出**：超过 32KB 的 chat.message 溢出到 `blobs/chat.<event_id>.txt`

```python
lk = acquire_lockfile(lock, blocking=True)
try:
    with ledger_path.open("a", encoding="utf-8") as f:
        f.write(line + "\n")
finally:
    release_lockfile(lk)
_notify_append(out)  # 通知内存观察者
```

**Rust 对应**：`flock` + `OpenOptions::new().append(true)`

### 账本读取

- **Tail 读取**（二进制反向扫描）：`ledger.py:106-131`
- **Follow 跟踪**（类似 `tail -f`）：`ledger.py:133-179` — 检测 inode 变化和文件截断
- **全量迭代**：`kernel/inbox.py:17-31` — 逐行 `json.loads`

---

## 三、消息投递引擎

### 完整消息流转路径

```
1. MCP 工具 cccc_message_send
2. → handlers/cccc_messaging.py message_send()
3. → daemon IPC: {"op": "chat_send", ...}
4. → server.py dispatch_request()
5. → ledger.py append_event(kind="chat.message")
6. → EventBroadcaster.on_append() (内存广播)
7. → auto_wake_recipients() (唤醒停止的 Actor)
8. → THROTTLE.queue_message()
9. → automation 线程每秒 tick_delivery()
10. → flush_pending_messages()
11a. PTY Actor: pty_submit_text() → os.write(master_fd)
11b. Headless Actor: 通过 cccc_inbox_list 主动拉取
```

### DeliveryThrottle 节流策略

文件：`src/cccc/daemon/messaging/delivery.py:247-461`

```
首次投递（last_delivery_at is None）:
  → 从未尝试: 允许
  → 已尝试: elapsed_attempt >= 5s (RETRY_INTERVAL) 才允许

后续投递:
  → elapsed_delivery >= min_interval_seconds (默认 0s)
  → 且 elapsed_attempt >= 5s

失败重试: requeue_front() 放回队列头部
```

**Rust 对应**：`Arc<Mutex<VecDeque<PendingMessage>>>`

---

## 四、Agent 生命周期

### PTY Actor 状态

```
[不存在] → actor_start → [running] → 进程退出/stop → [不存在]
```

文件：`src/cccc/runners/pty.py` — `subprocess.Popen` + `pty.openpty()`

### Headless Actor 状态

```
idle → working → waiting → stopped
```

文件：`src/cccc/runners/headless.py:22` — Agent 通过 MCP 主动报告状态

### Group 状态（影响消息投递）

```
active  → 所有消息正常投递
idle    → chat.message + system.notify 允许，其他阻塞
paused  → 所有投递阻塞，消息累积在 inbox
```

### PTY vs Headless 对比

| 维度 | PTY Runner | Headless Runner |
|------|-----------|-----------------|
| 文件 | `runners/pty.py` | `runners/headless.py` |
| 进程 | `subprocess.Popen` + `pty.openpty()` | 无真实子进程，内存 Session |
| 投递 | 写入 PTY master fd (stdin) | Agent 通过 `cccc_inbox_list` 拉取 |
| 平台 | POSIX 仅 | 跨平台 |

### 自动恢复

文件：`src/cccc/daemon/server.py:789-797`

- daemon 启动时调用 `_maybe_autostart_running_groups()`
- 恢复所有 `running=true` 的 group 的 actors
- 清理残留 PTY state 文件（上次崩溃留下的）

---

## 五、MCP Server 实现

### 协议

文件：`src/cccc/ports/mcp/main.py:228-241`

- **stdio JSON-RPC 2.0**（每个 Agent 一个 MCP Server 子进程）
- 每行一个 JSON + `\n`

### 身份注入

```python
CCCC_GROUP_ID = os.environ["CCCC_GROUP_ID"]
CCCC_ACTOR_ID = os.environ["CCCC_ACTOR_ID"]
```

Daemon 启动 Actor 进程时注入这两个环境变量

### 调用链

```
tools/call → handle_tool_call(name, args)
  → _handle_cccc_namespace() / _handle_context_namespace() / ...
  → 具体 handler
  → _call_daemon_or_raise({"op":"...", "args":{...}})
  → daemon socket → dispatch_request() → append_event()
```

### IPC 消息格式

```python
# 请求
DaemonRequest { v:1, op: str, args: Dict }
# 响应
DaemonResponse { v:1, ok: bool, result: Dict, error: Optional[DaemonError] }
```

---

## 六、Rust 重实现对照表

| 组件 | CCCC Python | Rust 建议 |
|------|------------|-----------|
| 单实例锁 | `fcntl.flock(LOCK_EX\|LOCK_NB)` | `fs2::FileExt::try_lock_exclusive()` |
| IPC Socket | `socket.AF_UNIX` | `tokio::net::UnixListener` |
| 协议 | 单行 JSON + `\n` | `serde_json` + newline framing |
| 账本写入 | `flock` + `open("a")` | `flock` + `OpenOptions::append` |
| 账本 tail | 二进制反向扫描 | 自定义 reverse scan |
| PTY | `pty.openpty()` + `selectors` | `nix::pty::openpty()` + `tokio` |
| 消息节流 | `threading.Lock` + 队列 | `Arc<Mutex<>>` + `VecDeque` |
| 事件广播 | `queue.Queue` per subscriber | `tokio::sync::broadcast::channel` |
| MCP Server | stdio JSON-RPC | stdio + `serde_json` JSON-RPC 2.0 |
| 事件 ID | `uuid4().hex` | `uuid::Uuid::new_v4().simple()` |
| 时间戳 | ISO 8601 UTC microseconds | `chrono::Utc::now().to_rfc3339_opts(Micros, true)` |
