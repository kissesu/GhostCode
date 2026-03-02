# Team Research: GhostCode

## 增强后的需求

**项目名称**: GhostCode（融合 CCCC + ccg-workflow + oh-my-claudecode 精华的全新开源项目）

**目标**: 构建一个基于 Rust 核心 + TypeScript 薄壳的多 Agent 协作开发平台，作为 Claude Code Plugin 分发。融合三个项目的核心优势：
- CCCC 的通信内核（Daemon + 消息可靠投递 + 多 Runtime 支持）
- ccg-workflow 的代码安全策略（Claude 独占写入 + 前端→Gemini/后端→Codex 智能路由 + team-*/spec-* 流程）
- oh-my-claudecode 的用户体验（Magic Keywords + Ralph 验证循环 + HUD 状态栏）

**技术栈**: Rust (核心引擎) + TypeScript (Claude Code Plugin 薄壳)

**MVP 范围**: Phase 1 = 通信内核 + Agent 管理

**发布形态**: Claude Code Plugin (开源)

---

## 约束集

### 硬约束

- [HC-1] **Claude Code Plugin 入口必须是 TS/JS** — Claude Code 插件系统只接受 JS 模块作为 Hook/Skill/Agent 入口。Rust 核心必须通过子进程或 IPC 与 TS 层通信。 — 来源：Claude Code Plugin 架构
- [HC-2] **单写者原则** — 通信内核必须采用单写者模型（借鉴 CCCC 的 append-only 事件账本），消除多 Agent 并发写入导致的竞态条件。 — 来源：CCCC 架构
- [HC-3] **Claude 独占文件写入权** — 所有文件系统写操作必须由 Claude Code 执行，外部模型（Codex/Gemini）只能输出 Diff/建议。注：ccg-workflow 通过提示词约定实现（非代码级沙箱），GhostCode 应考虑是否做代码级强制。 — 来源：ccg-workflow `templates/commands/workflow.md:189`
- [HC-4] **消息投递保证** — Agent 间消息必须具备：送达确认、已读回执、超时催促机制。不允许消息静默丢失。 — 来源：CCCC 消息语义
- [HC-5] **MCP 协议兼容** — 必须暴露 MCP 工具接口，因为这是 Claude Code 生态的标准集成方式。 — 来源：CCCC 31 个 MCP 工具 (`src/cccc/ports/mcp/toolspecs.py`) + Claude Code 生态
- [HC-6] **跨平台单二进制** — Rust 核心必须编译为 macOS (arm64/x86_64) + Linux (x86_64) 单二进制，零运行时依赖。 — 来源：分发需求
- [HC-7] **Rust-TS 通讯协议** — Rust Daemon 与 TS Plugin 间需采用高效协议（JSON-RPC over stdio 或 Unix socket），HUD 刷新延迟 < 100ms。 — 来源：Gemini 分析

### 软约束

- [SC-1] **Magic Keywords 自然语言触发** — 用户可通过自然语言关键词（如 "autopilot:", "ralph:", "team N:"）触发编排模式，无需记忆 slash 命令。 — 来源：oh-my-claudecode
- [SC-2] **配置分层** — 全局配置 (~/) → 项目配置 (./) → 环境变量 → 命令行参数，后者覆盖前者。推荐 TOML 格式。 — 来源：ccg-workflow + Gemini 分析
- [SC-3] **统一命令注册表** — Slash 命令、Magic Keywords、MCP 工具统一抽象为 Action Unit，在 Rust 侧维护 Command Registry，TS 层启动时获取动态注册。 — 来源：Gemini 分析
- [SC-4] **Agent 可观测性** — 提供多层级状态反馈：HUD 终端状态栏 + CLI Spinner 实时进度 + Web Dashboard 历史追溯。 — 来源：三个项目综合
- [SC-5] **会话复用** — 外部模型调用支持 SESSION_ID 机制，避免重复推理，节省 tokens。 — 来源：ccg-workflow
- [SC-6] **12 Runtime 支持** — 至少支持 Claude Code、Codex CLI、Gemini CLI 三种核心 Runtime，架构上可扩展至更多。 — 来源：CCCC
- [SC-7] **7 项验证检查** — Ralph 模式提供 BUILD/TEST/LINT/FUNCTIONALITY/ARCHITECT/TODO/ERROR_FREE 自动验证循环。 — 来源：oh-my-claudecode
- [SC-8] **事件溯源持久化** — 所有消息和状态变更以事件形式追加存储，支持完整重放和审计。 — 来源：CCCC append-only 设计

### 依赖关系

- [DEP-1] 通信内核 → Agent 生命周期管理：消息路由依赖 Agent 注册表
- [DEP-2] 多模型路由 → 通信内核：路由决策需要通过通信层分发到对应 Agent
- [DEP-3] Ralph 验证 → Agent 管理：验证循环需要启动/停止 Agent 并收集结果
- [DEP-4] Magic Keywords → 命令注册表：关键词匹配需要查询可用命令/Skill
- [DEP-5] HUD → Rust Daemon：状态推送依赖 IPC 通道
- [DEP-6] TS Plugin 层 → Rust 二进制：Plugin 安装时需要自动下载对应平台的 Rust 二进制

### 风险

- [RISK-1] **Claude Code Hooks API 变更** — Claude Code 的 Hook 系统尚在实验阶段，API 可能变化。缓解：TS 层做适配器模式，核心逻辑在 Rust 侧。
- [RISK-2] **Rust Daemon 僵尸进程** — Daemon 常驻内存，如果 Claude Code 会话异常退出，Daemon 可能变成僵尸。缓解：实现心跳检测 + 自动清理。
- [RISK-3] **多 Runtime CLI 版本碎片化** — Codex CLI/Gemini CLI 版本更新频繁，输出格式可能变化。缓解：每个 Runtime 做版本适配层。
- [RISK-4] **开发周期过长导致烂尾** — 14000 行核心代码，个人开发者可能失去动力。缓解：严格 MVP 分阶段，每个阶段独立可用。

---

## 成功判据

- [OK-1] Rust Daemon 启动后，Claude Code 和 Codex CLI 可作为 Actor 注册，互相发送消息并收到已读回执
- [OK-2] 消息持久化到本地事件账本，Daemon 重启后消息历史完整恢复
- [OK-3] Agent 异常退出时，Daemon 在 5s 内检测到并通知 Foreman
- [OK-4] TS Plugin 通过 IPC 调用 Rust Daemon，延迟 < 100ms
- [OK-5] 单二进制在 macOS arm64 上可直接运行，无需安装额外依赖
- [OK-6] `pip install cccc-pair` 级别的安装体验：Plugin 安装后自动下载 Rust 二进制

---

## 开放问题（已解决）

- Q1: 技术栈选择？ → A: Rust 核心 + TS 薄壳 → 约束：[HC-1], [HC-6]
- Q2: MVP 范围？ → A: Phase 1 通信内核 + Agent 管理 → 约束：[DEP-1]
- Q3: 产出形态？ → A: Claude Code Plugin → 约束：[HC-1], [HC-5]
- Q4: 核心功能优先级？ → A: 全部都要（通信 + 安全 + 验证 + Keywords + team/spec） → 分阶段实现
- Q5: 项目命名？ → A: GhostCode
- Q6: 是否需要 IM 桥接？ → A: 不需要，移除该功能

## 开放问题（待解决）
- Q6: HUD 在不同 Terminal (iTerm2 vs VSCode Terminal vs Ghostty) 中的渲染兼容方案？
- Q7: Skill Learning 生成的模式存放在 Rust 层（跨项目共享）还是 TS Plugin 层（跟随会话）？

---

## 建议的分阶段路线图

### Phase 1: 通信内核 + Agent 管理 (MVP)
- Rust Daemon（tokio 异步运行时）
- Append-only 事件账本
- Agent 注册/发现/生命周期管理
- 消息投递 + 已读回执 + 超时催促
- MCP 工具接口（核心 7-10 个）
- TS Plugin 壳（IPC 桥接 + 基础 Hook 注册）

### Phase 2: 多模型路由 + 代码安全
- Claude 独占写入权执行层
- 前端→Gemini / 后端→Codex 智能路由
- 会话复用 (SESSION_ID)
- codeagent-wrapper 等价物（Rust 实现）

### Phase 3: 验证引擎 + 用户体验
- Ralph 7 项验证循环
- Magic Keywords 解析器
- HUD 状态栏
- 成本追踪

### Phase 4: 高级功能
- Web Dashboard
- Skill Learning
- team-*/spec-* 流程

---

*研究完成时间: 2026-02-28*
*分析来源: Gemini 前端分析 + Claude 综合研判*
*Codex 后端分析: 失败（Codex CLI 异常退出），由 Claude 补充*

---

## 参考项目源码路径

所有功能参考、逻辑借鉴必须从以下本地源码中查询验证，禁止臆想、胡说、造假：

```
/Users/oi/CodeCoding/Code/github/claude-plugin/cccc/           -- CCCC 源码
/Users/oi/CodeCoding/Code/github/claude-plugin/ccg-workflow/    -- ccg-workflow 源码
/Users/oi/CodeCoding/Code/github/claude-plugin/oh-my-claudecode/ -- oh-my-claudecode 源码
```

注意：本研究文件中的技术描述基于 GitHub README 和 Gemini 分析产出。
进入实施阶段后，所有约束条目必须通过源码阅读重新验证。
