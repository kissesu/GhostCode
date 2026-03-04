<!-- @file GhostCode 系统架构说明 -->
<!-- @author Atlas.oi -->
<!-- @date 2026-03-04 -->

# GhostCode 系统架构说明

> 作者：Atlas.oi
> 日期：2026-03-04

## 系统架构概览

GhostCode 采用「Rust 核心 + TypeScript 薄壳」的分层架构，以 Unix Socket 为边界分离插件层与守护进程层，通过 NDJSON 账本实现数据持久化，并通过 HTTP + SSE 向 Web Dashboard 推送实时数据。

```
+------------------------------------------------------------------+
|                      Claude Code 进程                             |
|                                                                  |
|   Claude Code (CLI)  <--Hook 回调-->  GhostCode Plugin (TS)      |
+----------------------------------+-------------------------------+
                                   |
                    Unix Socket JSON-RPC（换行分隔）
                                   |
+----------------------------------v-------------------------------+
|                    GhostCode Daemon 进程 (Rust)                   |
|                                                                  |
|  Unix Socket 监听 --> 请求分发框架 (40 ops)                       |
|    |                                                             |
|    +-> Actor 管理 (生命周期：add/start/stop/remove)               |
|    +-> 消息投递引擎 (send/reply/inbox)                            |
|    +-> 验证引擎 (Ralph 循环，7 项检查)                            |
|    +-> 路由引擎 (多模型调度：Claude/Codex/Gemini)                 |
|    +-> Skill Learning (片段评分 + hash 去重 + 候选提升)            |
|    +-> Dashboard 查询 (快照 + 时间线)                             |
|    +-> SovereigntyGuard (执行期写入阻断)                          |
|    +-> diagnostics (health/ready/degraded/down)                 |
+----------------------------------+-------------------------------+
                                   |
                  append_event / read_events (flock)
                                   |
+----------------------------------v-------------------------------+
|                        存储层                                     |
|                                                                  |
|  ~/.ghostcode/groups/<group-id>/ledger.jsonl                     |
|  (NDJSON, append-only, flock 写锁，单写者原则)                    |
+----------------------------------+-------------------------------+
                                   |
                         HTTP + SSE (:7070)
                                   |
+----------------------------------v-------------------------------+
|                    ghostcode-web (axum 0.7)                      |
|                                                                  |
|  /dashboard  /timeline  /agents  /skills  /events (SSE)          |
|  /health  /metrics                                               |
|  /gc-web  --> Dashboard SPA (React + Tailwind CSS)               |
+------------------------------------------------------------------+
```

## 数据流图

### Plugin -> Daemon -> Ledger -> Dashboard

```
用户输入 prompt（含 Magic Keyword）
  |
  v
Claude Code：UserPromptSubmit Hook
  |
  v
Plugin (TS)：detectMagicKeywords()
  |
  v
Plugin (TS)：JSON-RPC over Unix Socket
  |
  v
Daemon (Rust)：dispatch(op="actor_start")
  |
  +-> SovereigntyGuard 检查（PreToolUse Hook）
  |
  v
Daemon (Rust)：append_event(ActorStart) 写账本
  |
  v
ghostcode-web：SSE tail 推送新事件
  |
  v
Dashboard SPA：接收 SSE，更新时间轴
```

### Skill Learning 数据流

```
Plugin (TS)：appendSessionContent(prompt)
  |（会话缓冲积累内容）
  v
Plugin (TS)：onSessionEnd()
  |
  v
Daemon (Rust)：skill_extract 请求
  |
  v
SkillLearning 引擎：评分 + hash 去重
  |（confidence >= 70 才记录）
  v
Ledger：append_event(SkillLearned)
  |
  v
SkillLearning 引擎：候选提升检查
  |（出现次数 >= 阈值则提升）
  v
Ledger：append_event(SkillPromoted)
```

## 配置分层

GhostCode 采用四层 TOML 配置，优先级从低到高依次为：

```
优先级（低 -> 高）
  default  <  global  <  project  <  runtime
```

### 四层说明

| 层级 | 文件路径 | 说明 |
|------|---------|------|
| **default** | 编译时内置 | 代码中硬编码的默认值，作为最终兜底 |
| **global** | `~/.ghostcode/config.toml` | 用户全局配置，适用于所有项目 |
| **project** | `./.ghostcode.toml` | 当前项目配置，随代码库版本控制 |
| **runtime** | 环境变量 `GHOSTCODE_*` / CLI flags | 运行时覆盖，优先级最高 |

### 配置合并规则

各层配置会按优先级从低到高逐层合并。后层的同名字段会覆盖前层，未设置的字段则继承前层值。

```toml
# ~/.ghostcode/config.toml（全局层示例）
[daemon]
socket_path = "/tmp/ghostcoded.sock"
request_timeout_secs = 30

[sovereignty]
enabled = true
protected_extensions = [".rs", ".ts"]

[web]
bind = "127.0.0.1:7070"
```

```toml
# ./.ghostcode.toml（项目层示例，覆盖全局设置）
[project]
name = "my-project"

[sovereignty]
# 项目额外保护的扩展名（追加到全局列表）
protected_extensions = [".rs", ".ts", ".go", ".py"]
```

### 运行时覆盖（最高优先级）

```bash
# 通过环境变量覆盖配置
GHOSTCODE_DAEMON_SOCKET_PATH=/custom/path.sock ghostcoded

# 通过 CLI flags 覆盖
ghostcoded --bind 0.0.0.0:7071
```

## 代码主权约束（SovereigntyGuard）

SovereigntyGuard 是 GhostCode 的核心安全机制，在**执行期**阻断非授权写入操作。

### 工作原理

```
Claude Code 执行 Write/Edit 操作
  |
  v
PreToolUse Hook 触发
  |
  v
Plugin (TS)：提取 tool_name + file_path
  |
  v
SovereigntyGuard 检查：
  1. 文件扩展名是否在 protected_extensions 中？
  2. 当前操作者是否为 Claude（主模型）？
  3. 是否在授权的 group_id 范围内？
  |
  +-- [通过] -> 允许写入，记录审计事件
  |
  +-- [拦截] -> 阻断写入，返回 sovereignty_violation 错误
                记录 violation 事件到账本
```

### 主权约束规则

- **Claude 独占写入**：受保护文件只允许 Claude 主模型写入
- **外部模型只读**：Codex / Gemini 等外部模型只能读取代码，不能直接修改
- **实时阻断**：违规写入在执行前被拦截，不会产生部分修改的文件
- **完整审计**：所有检查结果（通过或拦截）均记录到事件账本

### 配置主权约束

```toml
# ./.ghostcode.toml
[sovereignty]
# 是否启用主权约束（默认 true）
enabled = true

# 受保护的文件扩展名列表
protected_extensions = [".rs", ".ts", ".go", ".py", ".java"]

# 受保护的文件路径前缀（支持 glob）
protected_paths = ["src/", "crates/"]

# 是否对 violation 触发告警
alert_on_violation = true
```

## MCP 工具注册表

GhostCode Phase 9 实现了基于 `ToolDescriptor` 模式的 MCP 工具注册表，共注册 16 个标准工具。

### ToolDescriptor 模式

传统的线性 `match` 分发模式随工具数量增长会变得难以维护：

```rust
// 旧模式：线性 match，难以扩展
match tool_name {
    "actor_start" => handle_actor_start(params),
    "actor_stop" => handle_actor_stop(params),
    // ... 每新增工具都要修改此处
}
```

`ToolDescriptor` 模式将工具定义与分发逻辑分离：

```rust
// ToolDescriptor 结构体
pub struct ToolDescriptor {
    // 工具名称（MCP 标准名称）
    pub name: &'static str,
    // 工具描述（展示给 Claude）
    pub description: &'static str,
    // 输入 schema（JSON Schema 格式）
    pub input_schema: &'static str,
    // 处理函数指针
    pub handler: fn(params: Value) -> Result<Value>,
}

// 注册表：通过 Vec<ToolDescriptor> 驱动分发
pub static TOOL_REGISTRY: &[ToolDescriptor] = &[
    ToolDescriptor { name: "gc_ping", ... },
    ToolDescriptor { name: "gc_actor_start", ... },
    // ... 新增工具只需在此追加
];
```

### 已注册的 16 个 MCP 工具

| 工具名 | 说明 |
|--------|------|
| `gc_ping` | 检查 Daemon 连接状态 |
| `gc_actor_start` | 启动一个 Actor |
| `gc_actor_stop` | 停止一个 Actor |
| `gc_actor_list` | 列出所有 Actor |
| `gc_group_create` | 创建一个 Group |
| `gc_group_show` | 查看 Group 状态 |
| `gc_send_message` | 向 Actor 发送消息 |
| `gc_inbox_list` | 查看消息收件箱 |
| `gc_skill_list` | 列出 Skill 候选 |
| `gc_skill_promote` | 提升 Skill 到已确认 |
| `gc_skill_reject` | 拒绝 Skill 候选 |
| `gc_team_skill_list` | 跨 Group 聚合 Skill 列表 |
| `gc_dashboard` | 获取 Dashboard 快照 |
| `gc_timeline` | 获取事件时间线 |
| `gc_health` | 查看系统健康状态 |
| `gc_doctor` | 执行诊断检查 |

### team_skill_list 跨 Group 聚合

`gc_team_skill_list` 工具支持跨多个 Group 聚合 Skill 列表，用于在 team 模式下共享技能：

```json
// 请求示例
{
  "tool": "gc_team_skill_list",
  "params": {
    "group_ids": ["group-a", "group-b", "group-c"],
    "status": "promoted",
    "limit": 50
  }
}

// 响应示例
{
  "skills": [
    {
      "id": "skill-001",
      "name": "错误处理模式",
      "group_id": "group-a",
      "confidence": 0.92,
      "status": "promoted"
    }
  ],
  "total": 12,
  "groups_queried": 3
}
```

## 各 Crate 职责与依赖关系

```
ghostcode-types  <-- 共享类型定义（被所有 crate 引用）
    ^
    |------ ghostcode-ledger  (账本 IO + 查询)
    |------ ghostcode-router  (多模型路由引擎)
    |
    +------ ghostcode-daemon  (守护进程核心)
    |           依赖: types + ledger + router
    |
    +------ ghostcode-mcp     (MCP 工具注册表)
    |           依赖: types
    |
    +------ ghostcode-web     (HTTP Dashboard 服务器)
                依赖: types + ledger
```

### ghostcode-types

**职责**: 整个 workspace 的共享类型定义，所有 crate 的数据契约层。

| 模块 | 内容 |
|------|------|
| `event` | `Event` 结构体、`EventKind` 枚举（18 种事件类型） |
| `ipc` | `DaemonRequest` / `DaemonResponse` IPC 协议类型 |
| `actor` | Actor 定义（id、runtime、status） |
| `group` | Group 定义（id、name、state） |
| `addr` | Unix Socket 地址类型 |
| `dashboard` | Dashboard DTO（`LedgerTimelineItem`、`AgentStatusView`、`DashboardSnapshot`、`TimelinePage`） |
| `skill` | Skill Learning 类型（`SkillMetadata`、`LearnedSkill`、`PatternDetection`） |

### ghostcode-ledger

**职责**: 事件持久化存储，保证写入原子性和读取一致性。

核心函数：
- `append_event()` - flock 写锁 + append 模式原子追加
- `read_last_lines()` - 4KB 块反向扫描，高效读取最近 N 条
- `iter_events()` - 全量迭代，损坏行自动跳过（[ERR-1] 容错）
- `count_events()` - 统计有效事件数量

查询模块 (`query`)：
- 时间线分页查询
- Agent 状态聚合
- Dashboard 快照生成

### ghostcode-daemon

**职责**: GhostCode 的运行时核心，管理所有 Actor 的生命周期和通信。

子模块：

| 子模块 | 职责 |
|--------|------|
| `server` | Unix Socket 监听、连接管理、优雅关闭（30s 请求超时，2s 关闭等待） |
| `dispatch` | 40 个 op 的请求分发框架 |
| `actor_mgmt` | Actor 注册、启动、停止、移除 |
| `messaging` | send/reply/inbox 消息投递引擎 |
| `verification` | Ralph 验证循环（7 项自动验证） |
| `routing` | 多模型任务路由状态管理 |
| `skill_learning` | 片段评分、hash 去重、候选提升 |
| `dashboard` | Dashboard 数据查询接口 |
| `hud` | HUD 状态栏数据 |
| `lifecycle` | Daemon 启动与关闭流程 |
| `sovereignty` | SovereigntyGuard 执行期写入约束 |
| `diagnostics` | health endpoint + doctor 诊断 |

### ghostcode-router

**职责**: 多模型任务路由引擎，支持 DAG 拓扑排序和并行/顺序/回退策略。

- `dag` - 有向无环图拓扑排序（`TaskNode`、`topological_sort`）
- `task_format` - 任务格式解析

### ghostcode-web

**职责**: 独立 HTTP 服务器，向 Dashboard SPA 提供数据接口（默认绑定 `127.0.0.1:7070`）。

| 端点 | 方法 | 说明 |
|------|------|------|
| `/dashboard` | GET | DashboardSnapshot 快照（聚合视图） |
| `/timeline` | GET | 时间线分页查询（支持 cursor + limit） |
| `/agents` | GET | Agent 状态列表 |
| `/skills` | GET | Skill 候选列表 |
| `/events` | GET | SSE 实时事件流（tail NDJSON 账本） |
| `/health` | GET | 健康检查（ready / degraded / down） |
| `/metrics` | GET | 运行指标（事件数、Actor 数等） |
| `/gc-web` | GET | Dashboard SPA 入口 |

## 通信协议说明

### JSON-RPC over Unix Socket

Plugin（TypeScript）与 Daemon（Rust）通过 Unix Socket 通信，协议为换行分隔的 JSON-RPC。

**请求格式**：

```json
{
  "op": "actor_start",
  "params": {
    "group_id": "my-group",
    "actor_id": "worker-1"
  }
}
```

**响应格式（成功）**：

```json
{
  "ok": true,
  "data": {
    "actor_id": "worker-1",
    "status": "active"
  }
}
```

**响应格式（错误）**：

```json
{
  "ok": false,
  "error": "Actor worker-1 already running",
  "error_code": "GC_ACTOR_ALREADY_RUNNING"
}
```

### 已实现的操作（40 个）

| 类别 | 操作 |
|------|------|
| 核心 | ping, shutdown |
| Group 管理 | group_create, group_show, group_start, group_stop, group_delete, group_set_state, groups |
| Actor 管理 | actor_add, actor_list, actor_start, actor_stop, actor_remove |
| 消息 | send, reply, inbox_list, inbox_mark_read, inbox_mark_all_read |
| Headless | headless_status, headless_set_status |
| 路由（Phase 2） | route_task, route_task_parallel, route_status, route_cancel, session_list |
| 验证（Phase 3） | verification_start, verification_status, verification_cancel |
| HUD（Phase 3） | hud_snapshot, hud_update |
| Dashboard（Phase 4） | dashboard_snapshot, dashboard_timeline, dashboard_agents |
| Skill Learning（Phase 4） | skill_list, skill_extract, skill_promote, skill_reject, skill_get |

## 事件系统（EventKind 枚举）

账本中存储的所有事件均属于以下 18 种类型之一：

| 类别 | EventKind | 序列化值 |
|------|-----------|---------|
| Group 生命周期 | GroupCreate | `group.create` |
| | GroupUpdate | `group.update` |
| | GroupStart | `group.start` |
| | GroupStop | `group.stop` |
| | GroupSetState | `group.set_state` |
| Actor 生命周期 | ActorAdd | `actor.add` |
| | ActorUpdate | `actor.update` |
| | ActorStart | `actor.start` |
| | ActorStop | `actor.stop` |
| | ActorRemove | `actor.remove` |
| 消息 | ChatMessage | `chat.message` |
| | ChatRead | `chat.read` |
| | ChatAck | `chat.ack` |
| 系统 | SystemNotify | `system.notify` |
| Skill Learning | SkillLearned | `skill.learned` |
| | SkillPromoted | `skill.promoted` |
| | SkillRejected | `skill.rejected` |
| Dashboard | DashboardViewed | `dashboard.viewed` |

### 事件结构

```json
{
  "v": 1,
  "id": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4",
  "ts": "2026-03-04T10:30:00.000000Z",
  "kind": "actor.start",
  "group_id": "my-project",
  "scope_key": "default",
  "by": "user",
  "data": {}
}
```

字段说明：
- `v` - 协议版本号，固定为 1
- `id` - UUID v4 hex 格式（32 字符，无连字符）
- `ts` - ISO 8601 UTC 微秒精度时间戳
- `kind` - 事件类型（点分隔 snake_case）
- `group_id` - 所属 Group 标识
- `scope_key` - 作用域键
- `by` - 触发者（actor_id 或 "user"）
- `data` - 事件负载（任意 JSON 对象）

## 核心设计原则

1. **单写者原则** - Daemon 是唯一的状态写入者，消除竞态条件
2. **代码主权** - Claude 独占文件写入权，SovereigntyGuard 执行期阻断违规写入
3. **事件溯源** - 所有状态变更以不可变事件形式追加到账本，支持完整审计回溯
4. **协议优先** - 通过标准 JSON-RPC 接口通信，不依赖内部实现细节
5. **质量保证** - TDD + PBT 混合驱动开发：Red -> Green -> Refactor
6. **配置分层** - 四层 TOML 配置，灵活覆盖，不降级不回退
7. **注册表驱动** - MCP 工具通过 ToolDescriptor 注册表驱动，易于扩展
