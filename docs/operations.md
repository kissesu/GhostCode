<!-- @file GhostCode 运维手册 -->
<!-- @author Atlas.oi -->
<!-- @date 2026-03-04 -->

# GhostCode 运维手册

> 作者：Atlas.oi
> 日期：2026-03-04

本文档面向需要部署、维护和排障 GhostCode 的运维人员和开发者。

---

## 配置优先级与文件位置

### 四层配置优先级

GhostCode 使用四层 TOML 配置，优先级从低到高：

```
default（内置）< global（用户全局）< project（项目）< runtime（运行时）
```

后层配置覆盖前层的同名字段，未设置的字段继承前层值。

### 配置文件位置

| 层级 | 文件路径 | 修改方式 |
|------|---------|---------|
| default | 编译时内置，无文件 | 升级版本 |
| global | `~/.ghostcode/config.toml` | 手动编辑 |
| project | `./.ghostcode.toml`（项目根目录） | 手动编辑 |
| runtime | 环境变量 `GHOSTCODE_*` / CLI flags | 启动时传入 |

### 完整配置参考

```toml
# ~/.ghostcode/config.toml（全局配置示例）

[daemon]
# Unix Socket 路径
socket_path = "/tmp/ghostcoded.sock"
# 请求超时时间（秒）
request_timeout_secs = 30
# 优雅关闭等待时间（秒）
shutdown_wait_secs = 2
# Session Lease 超时（秒）：Daemon 持有锁的最长时间
session_lease_secs = 300

[web]
# Web Dashboard 绑定地址
bind = "127.0.0.1:7070"
# SSE 推送间隔（毫秒）
sse_poll_interval_ms = 500

[sovereignty]
# 是否启用代码主权约束
enabled = true
# 受保护的文件扩展名
protected_extensions = [".rs", ".ts", ".go", ".py"]
# violation 是否触发告警
alert_on_violation = true

[skill_learning]
# Skill 记录的最低置信度阈值（0-100）
confidence_threshold = 70
# Skill 提升为 promoted 所需的出现次数
promotion_threshold = 3

[logging]
# 日志级别：error / warn / info / debug / trace
level = "info"
# 日志格式：json / text
format = "json"
```

### 运行时环境变量覆盖

所有配置项均可通过环境变量覆盖，命名规则：`GHOSTCODE_<SECTION>_<KEY>` 全大写。

```bash
# 覆盖 socket 路径
GHOSTCODE_DAEMON_SOCKET_PATH=/custom/path.sock ghostcoded

# 覆盖日志级别
GHOSTCODE_LOGGING_LEVEL=debug ghostcoded

# 覆盖 Web 绑定地址
GHOSTCODE_WEB_BIND=0.0.0.0:7071 ghostcoded

# CI 环境跳过交互式提示
GHOSTCODE_NON_INTERACTIVE=1 ghostcode init
```

---

## 健康检查

### health endpoint

GhostCode Web 服务提供标准健康检查端点，适合集成到监控系统和负载均衡器。

```bash
# 基础健康检查
curl http://localhost:7070/health
```

**响应状态**：

| 状态 | HTTP 码 | 说明 |
|------|---------|------|
| `ready` | 200 | 所有组件正常运行 |
| `degraded` | 200 | 部分组件异常，核心功能可用 |
| `down` | 503 | 服务不可用 |

**响应示例（ready）**：

```json
{
  "status": "ready",
  "version": "0.5.0",
  "uptime_secs": 3600,
  "components": {
    "daemon": "ok",
    "ledger": "ok",
    "web": "ok"
  },
  "ts": "2026-03-04T10:30:00Z"
}
```

**响应示例（degraded）**：

```json
{
  "status": "degraded",
  "version": "0.5.0",
  "components": {
    "daemon": "ok",
    "ledger": "degraded",
    "web": "ok"
  },
  "issues": [
    "ledger: 最近 5 条事件中有 1 条解析失败（[ERR-1]），已自动跳过"
  ]
}
```

### 监控指标

```bash
# 获取运行指标（Prometheus 格式）
curl http://localhost:7070/metrics
```

主要指标：

```
# 账本总事件数
ghostcode_ledger_events_total{group_id="my-project"} 1234

# 活跃 Actor 数量
ghostcode_actors_active 3

# Skill 候选数量
ghostcode_skills_pending 7

# 请求处理延迟（毫秒）
ghostcode_request_duration_ms{op="actor_start",status="ok"} 2.5
```

---

## 诊断命令：ghostcode doctor

`ghostcode doctor` 执行五类检查，帮助快速定位问题。

### 五类检查详解

#### 检查 1：二进制安装

验证 Daemon 二进制是否正确安装，版本是否匹配。

```bash
# 检查项目
# - ~/.ghostcode/bin/ghostcoded 是否存在
# - 文件是否可执行
# - 版本是否与 npm 包版本匹配
# - SHA256 checksum 是否验证通过
```

#### 检查 2：连接状态

验证 Daemon 是否运行，Plugin 能否成功连接。

```bash
# 检查项目
# - ~/.ghostcode/daemon/ghostcoded.addr.json 是否存在
# - addr.json 中的 PID 进程是否存在
# - Unix Socket 文件是否存在
# - 发送 ping 请求是否成功（超时 3 秒）
```

#### 检查 3：配置文件

验证配置文件是否合法。

```bash
# 检查项目
# - ~/.ghostcode/config.toml 是否存在（不存在则使用默认配置，不报错）
# - ./.ghostcode.toml 是否合法 TOML（语法检查）
# - .mcp.json 是否存在且格式正确
```

#### 检查 4：权限

验证文件系统权限是否正确。

```bash
# 检查项目
# - ~/.ghostcode/ 目录是否可写
# - ledger 目录是否可写
# - Socket 文件权限是否允许当前用户访问（600 或 660）
```

#### 检查 5：僵尸进程

检测是否存在遗留的 Daemon 进程或 Socket 文件。

```bash
# 检查项目
# - 是否有多个 ghostcoded 进程在运行
# - addr.json 中的 PID 是否与实际运行进程匹配
# - Socket 文件是否有对应的监听进程
```

### doctor 命令选项

```bash
# 标准诊断（文本输出）
ghostcode doctor

# JSON 格式输出（适合 CI / 脚本解析）
ghostcode doctor --json

# 自动修复可修复的问题（重新安装、清理僵尸进程等）
ghostcode doctor --fix

# 仅检查特定类别
ghostcode doctor --check binary
ghostcode doctor --check connection
ghostcode doctor --check config
ghostcode doctor --check permissions
ghostcode doctor --check zombie
```

---

## 僵尸进程恢复策略

GhostCode 在 Phase 8 引入了完整的僵尸进程检测与恢复机制。

### 什么情况会产生僵尸进程

1. Daemon 进程被强制杀死（kill -9），Socket 文件和 addr.json 未清理
2. 系统崩溃后重启，遗留上次会话的临时文件
3. 多个 GhostCode 实例并发启动产生竞态

### 自动恢复流程

Daemon 启动时会自动执行以下检查：

```
ghostcoded 启动
  |
  +-- 1. 读取 addr.json
  |       存在? -> 检查 PID 是否存活
  |                  存活? -> 已有实例运行，当前进程退出
  |                  不存活? -> 判定为僵尸，执行清理
  |       不存在? -> 继续正常启动
  |
  +-- 2. 僵尸清理
  |       删除旧的 Socket 文件
  |       删除旧的 addr.json
  |
  +-- 3. 竞态锁保护（Session Lease）
  |       写入临时锁文件（含 PID + 时间戳）
  |       如果 lease 超时（默认 300s），自动释放锁
  |
  +-- 4. 正常启动
          绑定 Unix Socket
          写入新的 addr.json
```

### 手动清理

如果自动恢复失败，可手动清理：

```bash
# 查找僵尸进程
ps aux | grep ghostcoded

# 强制终止所有 ghostcoded 进程
pkill -f ghostcoded

# 清理临时文件
rm -f /tmp/ghostcoded.sock
rm -f ~/.ghostcode/daemon/ghostcoded.addr.json
rm -f ~/.ghostcode/daemon/ghostcoded.lock

# 重新启动
ghostcoded &
# 或者让 Plugin 自动启动
```

### Session Lease 机制

Session Lease 防止 Daemon 因异常情况长期持有独占锁：

- Daemon 启动时写入 lease 文件（含过期时间戳）
- 如果进程正常运行，定期刷新 lease 时间戳
- 如果进程崩溃，lease 文件在超时后失效
- 新的 Daemon 启动时检测 lease 是否超时，超时则强制清理

---

## 错误码参考

GhostCode 使用结构化错误码，格式为 `GC_<CATEGORY>_<SPECIFIC>`。

### IPC 错误（GC_IPC_xxx）

| 错误码 | 说明 | 常见原因 |
|--------|------|---------|
| `GC_IPC_CONNECT_FAILED` | 无法连接到 Daemon | Daemon 未运行，Socket 文件不存在 |
| `GC_IPC_TIMEOUT` | 请求超时 | Daemon 过载，请求处理时间 > 30s |
| `GC_IPC_INVALID_REQUEST` | 请求格式错误 | JSON 格式不合法，缺少必填字段 |
| `GC_IPC_UNKNOWN_OP` | 未知操作码 | op 字段拼写错误 |
| `GC_IPC_SOCKET_BROKEN` | Socket 连接断开 | Daemon 崩溃或被强制终止 |

### 配置错误（GC_CONFIG_xxx）

| 错误码 | 说明 | 常见原因 |
|--------|------|---------|
| `GC_CONFIG_PARSE_ERROR` | 配置文件解析失败 | TOML 语法错误 |
| `GC_CONFIG_INVALID_VALUE` | 配置值不合法 | 类型错误或超出范围 |
| `GC_CONFIG_FILE_NOT_FOUND` | 配置文件不存在 | 仅在明确指定路径时报错 |
| `GC_CONFIG_PERMISSION_DENIED` | 无权限读取配置文件 | 文件权限设置错误 |

### Actor 错误（GC_ACTOR_xxx）

| 错误码 | 说明 | 常见原因 |
|--------|------|---------|
| `GC_ACTOR_NOT_FOUND` | Actor 不存在 | actor_id 拼写错误 |
| `GC_ACTOR_ALREADY_RUNNING` | Actor 已在运行 | 重复调用 actor_start |
| `GC_ACTOR_NOT_RUNNING` | Actor 未运行 | 对已停止的 Actor 发送消息 |

### 主权约束错误（GC_SOVEREIGNTY_xxx）

| 错误码 | 说明 | 常见原因 |
|--------|------|---------|
| `GC_SOVEREIGNTY_VIOLATION` | 写入操作被阻断 | 非授权操作者尝试写入受保护文件 |
| `GC_SOVEREIGNTY_DISABLED` | 主权约束未启用 | 配置中 `enabled = false` |

### 账本错误（GC_LEDGER_xxx）

| 错误码 | 说明 | 常见原因 |
|--------|------|---------|
| `GC_LEDGER_WRITE_FAILED` | 账本写入失败 | 磁盘空间不足，权限错误 |
| `GC_LEDGER_CORRUPT_LINE` | 账本行解析失败（[ERR-1]） | 非正常关闭导致行不完整 |
| `GC_LEDGER_LOCK_TIMEOUT` | 获取写锁超时 | 高并发写入 |

### 安装错误（GC_INSTALL_xxx）

| 错误码 | 说明 | 常见原因 |
|--------|------|---------|
| `GC_INSTALL_CHECKSUM_MISMATCH` | checksum 校验失败 | 下载文件损坏，网络问题 |
| `GC_INSTALL_BINARY_NOT_FOUND` | 二进制文件不存在 | 未执行 ghostcode init |
| `GC_INSTALL_PERMISSION_DENIED` | 无权限安装 | ~/.ghostcode/bin/ 权限问题 |

---

## 日志和可观测性

### 日志配置

```toml
[logging]
# 日志级别
level = "info"  # error / warn / info / debug / trace

# 日志格式
format = "json"  # json（结构化，推荐生产使用）或 text（可读，推荐开发使用）

# 日志输出目标
# stdout：标准输出（默认）
# file：写入文件
output = "stdout"

# 文件日志路径（output = "file" 时使用）
# file_path = "~/.ghostcode/logs/ghostcoded.log"
```

### 结构化日志示例

```json
{
  "ts": "2026-03-04T10:30:00.123456Z",
  "level": "info",
  "msg": "收到请求",
  "op": "actor_start",
  "group_id": "my-project",
  "actor_id": "worker-1",
  "latency_ms": 2
}
```

### 开发调试模式

```bash
# 启用 debug 日志（包含详细的请求/响应内容）
GHOSTCODE_LOGGING_LEVEL=debug ghostcoded

# 启用 trace 日志（包含所有内部状态变更，非常详细）
GHOSTCODE_LOGGING_LEVEL=trace ghostcoded
```

### 账本作为审计日志

所有状态变更都记录在账本中，可以作为审计日志使用：

```bash
# 查看最近 20 条事件
tail -20 ~/.ghostcode/groups/<group-id>/ledger.jsonl | jq .

# 查看所有 actor_start 事件
grep '"kind":"actor.start"' ~/.ghostcode/groups/<group-id>/ledger.jsonl | jq .

# 查看主权约束违规记录
grep '"kind":"system.notify"' ~/.ghostcode/groups/<group-id>/ledger.jsonl \
  | jq 'select(.data.type == "sovereignty_violation")'
```

### 实时事件流监控

```bash
# 通过 SSE 实时监控事件流
curl -N http://localhost:7070/events

# 过滤特定事件类型（借助 grep）
curl -N http://localhost:7070/events | grep "skill.learned"
```

---

## 生产部署建议

### 权限最小化

```bash
# 创建专用用户（可选，生产环境推荐）
sudo useradd -r -s /bin/false ghostcode

# 设置 ~/.ghostcode 目录权限
chmod 700 ~/.ghostcode
chmod 600 ~/.ghostcode/config.toml

# Socket 文件权限（只允许当前用户访问）
# ghostcoded 会自动设置，无需手动操作
```

### 进程守护

推荐使用 systemd 或 launchd 管理 Daemon 进程（生产环境）：

```ini
# /etc/systemd/system/ghostcoded.service 示例
[Unit]
Description=GhostCode Daemon
After=network.target

[Service]
Type=simple
User=<your-user>
ExecStart=/home/<your-user>/.ghostcode/bin/ghostcoded
Restart=on-failure
RestartSec=5
Environment=GHOSTCODE_LOGGING_LEVEL=info
Environment=GHOSTCODE_LOGGING_FORMAT=json

[Install]
WantedBy=multi-user.target
```

### 磁盘空间管理

账本文件会随时间增长，建议定期轮转：

```bash
# 查看账本大小
du -sh ~/.ghostcode/groups/*/ledger.jsonl

# 手动轮转（归档旧账本）
# 注意：轮转时需确保 Daemon 未在写入（发送 shutdown 后轮转）
ghostcoded --shutdown
mv ~/.ghostcode/groups/<group>/ledger.jsonl \
   ~/.ghostcode/groups/<group>/ledger-$(date +%Y%m%d).jsonl
# 重启 Daemon
ghostcoded &
```
