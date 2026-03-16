# Changelog

本文件记录 GhostCode 项目的所有重要变更，遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 规范。
版本号遵循 [语义版本控制](https://semver.org/lang/zh-CN/)。

## [Unreleased]

_当前开发中的变更，将在下次版本发布时归入正式版本号。_

## [0.2.0] - 2026-03-16

### Added

- **Route 事件类型**: 新增 `route.start`/`route.complete`/`route.error` 三种 EventKind，记录多模型调用（Codex/Gemini/Claude）的完整生命周期
- **Wrapper 账本集成**: `ghostcode-wrapper` 新增 `--group-id` 参数，模型调用前后自动写入 route 事件到账本（best-effort，失败不阻塞）
- **SKILL.md 传递 group-id**: 所有 gc:* skill 的 wrapper 调用追加 `--group-id "$GHOSTCODE_GROUP_ID"` 参数
- **RouteTimelineEvent 组件**: Timeline 中 route 事件以复合状态展示（进行中脉冲动画、完成绿色、错误红色）
- **ActiveRoutesPanel 面板**: 实时展示当前所有进行中的 LLM 调用，含动态计时器
- **AgentGraph Querying 状态**: Agent 有活动 route 调用时显示紫色 "Querying" 状态
- **前端 Route 数据层**: `useDashboard` 新增 `activeRoutes` state，基于 `correlationId` 合并 start/complete/error 事件

### Fixed

- **Dashboard 账本路径 Bug**: `dashboard.rs` 中 `ledger_path()` 读取旧路径 `ledger.ndjson`，修正为 `state/ledger/ledger.jsonl`

## [0.1.3] - 2026-03-16

### Fixed

- **Dashboard 浏览器不自动打开**: 将 `openURL` 从 `ensureWeb()` 内部移至调用方，SessionStart hook 每次新会话主动打开浏览器
  - 根因：`ensureWeb()` 耦合了"确保服务运行"和"打开浏览器"两个职责，当 Web Server 已在运行（上一会话 detached 进程），`_doEnsureWeb` 检测到已运行后直接返回，不打开浏览器
  - `web.ts`: 移除 `_doEnsureWeb` 中的 `openURL` 调用，遵循单一职责原则
  - `hook-session-start.mjs`: `ensureWeb()` 之后主动 import 并调用 `openURL`

## [0.1.2] - 2026-03-16

### Fixed

- **Dashboard 自启动跨会话失效**: 移除 `hook-session-start.mjs` 中 `if (!state.webStarted)` 条件判断，改为每次 SessionStart 无条件调用 `ensureWeb()`
  - 根因：状态文件 `hook-state.json` 跨会话持久化，旧会话的 `webStarted=true` 导致新会话跳过 Dashboard 启动
  - `ensureWeb()` 内部已通过 health check 实现幂等，重复调用安全无副作用

## [0.1.1] - 2026-03-16

### Fixed

- **Dashboard 自启动时机**: 将 `ensureWeb()` 从 PreToolUse hook 移至 SessionStart hook，会话启动即自动启动 Dashboard Web 端，无需等待首次工具调用
  - `hook-session-start.mjs`: 转为 async 函数，新增 `ensureWeb()` 调用和状态文件读写
  - `hooks.json`: SessionStart 改用 `run.mjs` 包装器运行，超时从 5s 增至 15s
  - `handlers.ts`: 移除 `preToolUseHandler` 中冗余的 `ensureWeb()` 调用
  - `hook-pre-tool-use.mjs`: 保留 `state.webStarted` 保护的 fallback 路径

## [0.1.0] - 2026-03-15

首个功能完整版本，包含 Rust 核心引擎 + TypeScript Plugin 薄壳 + Web Dashboard 的完整架构。

### Added

#### Rust 核心引擎

- **ghostcode-daemon**: Unix Socket 服务器，支持并发连接、请求超时、优雅关闭
- **ghostcode-ledger**: append-only 事件账本，支持文件锁写入和时间范围查询
- **ghostcode-types**: 统一类型定义（Event、IPC 协议、配置结构）
- **ghostcode-router**: 多模型路由引擎
  - 三后端支持：Codex / Claude / Gemini
  - StreamParser 统一 JSON 流解析（含 Gemini role=assistant+delta 优先级修复）
  - ProcessManager 子进程管理（工作目录 + 环境变量传递）
  - SovereigntyGuard 代码主权守卫（Claude 独占写入策略）
  - SessionStore 会话 ID 持久化
- **ghostcode-mcp**: MCP Server，暴露 Daemon 能力为 Claude Code 工具
  - 基础工具：group_list/info、actor_start/stop/list、message_send、inbox 等
  - Session Gate 工具：session_gate_open/submit/close/abort（多模型审查门控）
- **ghostcode-web**: HTTP 服务器，提供 REST API + SSE 实时事件流
- **ghostcode-config**: TOML 配置分层加载（全局 + 项目级）
- **ghostcode-wrapper**: 薄 CLI 入口，统一调用三种 AI 后端
  - Gemini 沙箱隔离（tempfile::tempdir 专属目录）
  - ROLE_FILE 注入机制
  - stdin/args 双通道任务文本传递

#### TypeScript Plugin

- **Hook 系统**: PreToolUse / SessionStart / SessionEnd / SubagentStart/Stop / PreCompact
  - hook-pre-tool-use.mjs: Daemon + Web Dashboard 自动启动（跨进程幂等）
  - webStarted 状态跟踪，修复 Daemon 已运行时 ensureWeb 不调用的 bug
- **Magic Keywords**: gc-web（Dashboard 手动启动）
- **Session Lease**: 会话租约管理，支持多 Agent 协作
- **IPC 客户端**: Unix Socket JSON-RPC 通信
- **web.ts**: Dashboard 生命周期管理（ensureWeb 单实例保证）

#### 技能系统 (gc:*)

- **gc:research**: 多模型并行研究（代码库探索 + 约束集提取）
- **gc:plan**: 多模型协作规划（规模评估 + 并行分析 + 零决策实施计划）
- **gc:execute**: 智能执行引擎（Team Builder 并行 / TDD 串行自动选择）
- **gc:review**: 双模型交叉审查（Codex 后端 + Gemini 前端独立审查 + Session Gate 门控）

#### Session Gate（Phase 7）

- SessionGateStore: 多模型审查强制门控存储器
- 完整生命周期：open -> submit(per model) -> close（合并输出）/ abort（中止）
- 状态文件绑定 groups_dir 实例路径，避免多 Daemon 覆盖
- dispatch.rs 严格参数校验（required_models 非空字符串、data 必须为 Object）

#### Web Dashboard

- AgentGraph: Agent 拓扑可视化组件
- Timeline: 事件时间线组件（REST 快照 + SSE 实时合并）
- useSSE Hook: SSE 订阅 + 指数退避重连 + 类型守卫校验

#### 基础设施

- CI 流水线：Rust 构建/测试/clippy + TS 构建/测试
- Release 流水线：三平台矩阵构建（darwin-arm64/darwin-x64/linux-x64）+ npm publish + GitHub Release
- E2E 测试矩阵
- .geminiignore 项目级配置

### Fixed

- **C1**: ghostcode-wrapper Gemini 沙箱使用 tempfile::tempdir() 隔离，防止跨任务数据泄露
- **W1**: Session Gate 状态文件绑定 groups_dir 实例路径，避免多 Daemon 互相覆盖
- **W2**: required_models 参数严格校验非空字符串，拒绝静默丢弃无效元素
- **W3**: data 参数校验为 JSON Object 类型，防止非法输入
- **W4**: useSSE.ts 类型守卫替代 as string 断言，运行时校验 SSE 事件结构
- 并发竞态修复：Daemon 启动序列 + 事件广播通道
- 内存安全修复：所有权规则遵守 + 生命周期正确性
- 时间排序修复：账本查询按时间戳排序
- 冷启动自举修复：CLI doctor + 路径一致性
- 构建入口 + 仓库地址 + 发布包完整性修复
- Dashboard 自动启动修复：hook-pre-tool-use.mjs 逻辑流重构

### Changed

- 技能重构：spec-*/team-* 7 个旧技能合并为 gc:* 4 个统一技能
- Gemini stream 解析：role=assistant+delta 事件归类为 AgentMessage（非 Progress）
- ProcessManager: 支持工作目录和环境变量传递参数
- Release 流水线：扩展为三组件（ghostcoded + ghostcode-mcp + ghostcode-wrapper）

[Unreleased]: https://github.com/kissesu/GhostCode/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/kissesu/GhostCode/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/kissesu/GhostCode/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/kissesu/GhostCode/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/kissesu/GhostCode/releases/tag/v0.1.0
