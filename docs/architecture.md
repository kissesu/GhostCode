# GhostCode 架构设计

## 总体架构

```
+---------------------------------------------+
|         Claude Code Plugin (TS 薄壳)         |
|  Hooks + Skills + Agents + HUD 渲染          |
|  ~500 行 TS                                  |
+---------------------+-----------------------+
                      | JSON-RPC (stdio/socket)
+---------------------+-----------------------+
|              GhostCode Core (Rust)             |
|                                               |
|  +------------------+  +------------------+  |
|  |   通信内核        |  |   命令注册表      |  |
|  |   Daemon          |  |   Command        |  |
|  |   事件账本        |  |   Registry       |  |
|  |   消息投递        |  |                  |  |
|  +------------------+  +------------------+  |
|                                               |
|  +------------------+  +------------------+  |
|  |   Agent 管理      |  |   多模型路由      |  |
|  |   注册/发现       |  |   Claude 独占     |  |
|  |   生命周期        |  |   写入权          |  |
|  |   健康检查        |  |   前端/后端分流   |  |
|  +------------------+  +------------------+  |
|                                               |
|  +------------------+                        |
|  |   验证引擎        |                        |
|  |   Ralph 循环      |                        |
|  |   7 项检查        |                        |
|  +------------------+                        |
+-----------------------------------------------+
           |              |             |
     +-----+-----+  +----+----+  +-----+-----+
     | Claude    |  | Codex   |  | Gemini    |
     | Code      |  | CLI     |  | CLI       |
     | (Actor)   |  | (Actor) |  | (Actor)   |
     +-----------+  +---------+  +-----------+
```

## 分层职责

### Layer 1: TS Plugin 薄壳
- Claude Code Hook 注册（UserPromptSubmit / PreToolUse / PostToolUse / Stop）
- Magic Keywords 检测与路由
- HUD 状态栏渲染
- Skill / Agent 定义文件
- 启动/停止 Rust Daemon 子进程

### Layer 2: Rust Core Engine
- Daemon 守护进程（tokio 异步运行时）
- Append-only 事件账本（消息持久化）
- Agent 注册表 + 生命周期管理
- 消息路由 + 投递确认 + 已读回执
- 统一命令注册表（Slash/MCP/Keywords → Action Unit）
- 多模型路由策略（前端→Gemini / 后端→Codex）
- Ralph 验证循环引擎

### Layer 3: Runtime Actors
- 各 CLI 工具作为独立 Actor 运行
- 通过 MCP 工具与 Daemon 通信
- Claude Code 拥有写入权（Foreman 角色）
- 其他模型为只读顾问（Peer 角色）

## 核心设计原则

1. **单写者原则** - Daemon 是唯一的状态写入者，消除竞态
2. **代码主权** - Claude 独占文件写入权，外部模型只能建议
3. **事件溯源** - 所有状态变更以事件形式追加，支持重放
4. **协议优先** - 通过标准 MCP/JSON-RPC 接口通信，不依赖内部实现
5. **双阶段质量保证** - TDD+PBT 贯穿开发和验证：Agent 开发时写测试驱动实现，Ralph 验证时审计测试质量

## 分阶段路线图

### Phase 1: 通信内核 + Agent 管理 (MVP)
### Phase 2: 多模型路由 + 代码安全
### Phase 3: 验证引擎 + 用户体验
### Phase 4: 高级功能（Skill Learning / Web Dashboard 等）

详见 research.md 中的完整路线图。
