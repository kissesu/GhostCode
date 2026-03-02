# GhostCode 参考项目分析

> **重要**: 所有功能描述必须基于本地 clone 的源码验证，禁止臆想、胡说、造假。
> 进入实施阶段后，本文件中的「精华提取」列表需要逐项通过源码阅读确认。

## 本地源码路径

```
/Users/oi/CodeCoding/Code/github/claude-plugin/cccc/           -- CCCC
/Users/oi/CodeCoding/Code/github/claude-plugin/ccg-workflow/    -- ccg-workflow
/Users/oi/CodeCoding/Code/github/claude-plugin/oh-my-claudecode/ -- oh-my-claudecode
```

## 三个上游项目对比

| 维度 | CCCC | ccg-workflow | oh-my-claudecode |
|------|------|-------------|-----------------|
| GitHub | [ChesterRa/cccc](https://github.com/ChesterRa/cccc) | [fengshao1227/ccg-workflow](https://github.com/fengshao1227/ccg-workflow) | [Yeachan-Heo/oh-my-claudecode](https://github.com/Yeachan-Heo/oh-my-claudecode) |
| 本地路径 | `.../claude-plugin/cccc` | `.../claude-plugin/ccg-workflow` | `.../claude-plugin/oh-my-claudecode` |
| Stars | 432 | 2,704 | 7,748 |
| 许可证 | Apache-2.0 | MIT | MIT |
| 技术栈 | Python | Go + Markdown | TypeScript |
| 核心定位 | 通信内核 | 多模型协作 | Agent 编排 |
| 创建时间 | 2025-08-15 | 2026-01-04 | 2026-01-09 |

## 从每个项目提取的精华（已源码验证）

### 从 CCCC 提取

| # | 状态 | 功能 | 源码路径 | 说明 |
|---|------|------|---------|------|
| 1 | [x] 已验证 | Daemon 守护进程架构（单写者 + append-only 事件账本） | `src/cccc/kernel/ledger.py` (append-only, 文件锁互斥), `src/cccc/daemon/server.py` (单实例锁), `src/cccc/daemon_main.py` (启动入口) | 账本使用 `open("a")` 追加写入 + `acquire_lockfile` 文件锁保证单写者 |
| 2 | [x] 已验证 | Agent 注册/发现/生命周期管理 | `src/cccc/daemon/actors/actor_lifecycle_ops.py` (start/stop/restart), `src/cccc/kernel/actors.py` (find/list/update), `src/cccc/daemon/ops/registry_ops.py` (跨 Group 注册表) | 通过 `group.yaml` 持久化，Daemon 启动时自动恢复上次运行中的 Actor |
| 3 | [x] 已验证 | 消息可靠投递（已读回执 + 超时催促 + 注意力 ACK） | `src/cccc/daemon/messaging/delivery.py` (投递引擎/节流), `src/cccc/daemon/messaging/inbox_ack_ops.py` (已读游标 + attention ACK), `src/cccc/daemon/automation/engine.py` (三级 nudge 催促 + escalation) | `DeliveryThrottle` 管理队列，nudge 支持 reply_required / attention_ack / unread 三级超时 |
| 4 | [x] 已验证 | 12 种 Runtime 支持 | `src/cccc/daemon/server.py:210-225` (`SUPPORTED_RUNTIMES` 元组) | amp, auggie, claude, codex, cursor, droid, gemini, kilocode, neovate, opencode, copilot, custom；其中 7 种支持 MCP 自动安装 |
| 5 | [!] 有偏差 | ~~49~~ **31** 个 MCP 工具的命名空间设计 | `src/cccc/ports/mcp/toolspecs.py` (工具定义, 注释标注 "31 entries") | 实际 31 个工具，非 49 个；按前缀分约 11 个语义组 |
| 6 | [x] 已验证 | Headless 模式 | `src/cccc/runners/headless.py` (HeadlessSession 状态机: idle/working/waiting/stopped), `src/cccc/ports/mcp/handlers/headless.py` (MCP 接口) | 面向纯 MCP 驱动的无 PTY Agent，通过轮询 inbox 接收消息 |
| 7 | [x] 已验证 | Automation 规则引擎 | `src/cccc/daemon/automation/engine.py` (~1800行), `src/cccc/contracts/v1/automation.py` (规则契约) | 四层自动化：消息级催促 → 会话级空闲检测 → Help 刷新提醒 → 用户自定义规则(cron/interval/at) |

### 从 ccg-workflow 提取

| # | 状态 | 功能 | 源码路径 | 说明 |
|---|------|------|---------|------|
| 1 | [!] 有偏差 | Claude 独占文件写入权（代码主权） | `templates/commands/workflow.md:189`, `templates/commands/spec-impl.md:14,51` | **提示词层面的约定**，非代码级沙箱强制。命令模板要求外部模型只输出 diff patch，Claude 重写后落盘 |
| 2 | [x] 已验证 | 前端→Gemini / 后端→Codex 智能路由 | `src/utils/config.ts:75-93` (默认路由配置), `src/types/index.ts:14-30` (ModelRouting 接口), `templates/commands/spec-impl.md:33-35` (Route A/B 规则) | 配置层(TS) + 命令模板层双层实现；路由决策由 Claude 在执行时判断，非运行时自动 |
| 3 | [x] 已验证 | 会话复用（SESSION_ID 机制） | `codeagent-wrapper/parser.go:161` (统一提取), `codeagent-wrapper/main.go:483-485` (输出), `codeagent-wrapper/backend.go:97-101` (Claude resume), `codeagent-wrapper/config.go:266-279` (参数解析) | Go 实现，支持 Codex(`thread_id`) / Claude(`session_id`) / Gemini(`session_id`) 三种后端 |
| 4 | [x] 已验证 | 6 阶段工作流（研究→构思→计划→执行→优化→评审） | `templates/commands/workflow.md:106,117-183` | 6 个阶段各有独立模式标记：[模式：研究/构思/计划/执行/优化/评审] |
| 5 | [x] 已验证 | team-* 命令（4 个文件） | `templates/commands/team-research.md`, `team-plan.md`, `team-exec.md`, `team-review.md` | team-exec 需 `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`，Lead 只协调不写代码 |
| 6 | [x] 已验证 | spec-* 命令（5 个文件） | `templates/commands/spec-init.md`, `spec-research.md`, `spec-plan.md`, `spec-impl.md`, `spec-review.md` | 依赖第三方 `@fission-ai/openspec` CLI |
| 7 | [x] 已验证 | TOML 配置管理 | `src/utils/config.ts:5` (smol-toml 库), `src/types/index.ts:32-55` (CcgConfig 接口) | 配置路径 `~/.claude/.ccg/config.toml`，含 routing/workflows/paths/mcp/performance 节 |
| 8 | [x] 已验证 | codeagent-wrapper（Go 语言） | `codeagent-wrapper/main.go` (入口), `backend.go` (策略模式 Backend 接口), `executor.go` (并行执行 + DAG 拓扑排序), `parser.go` (JSON stream 解析), `config.go` (参数解析) | 零外部依赖纯标准库，预编译 6 平台二进制 |

### 从 oh-my-claudecode 提取

| # | 状态 | 功能 | 源码路径 | 说明 |
|---|------|------|---------|------|
| 1 | [x] 已验证 | Magic Keywords 自然语言触发 | `scripts/keyword-detector.mjs` (运行时), `src/hooks/keyword-detector/index.ts` (TS 源码) | 15 个关键词分组：ralph/autopilot/ultrawork/ulw/team/pipeline/ccg/ralplan/tdd/deepsearch 等，注册在 `UserPromptSubmit` Hook |
| 2 | [!] 有偏差 | 三层 Skill 架构（Execution + Enhancement + Guarantee） | `docs/ARCHITECTURE.md:45-104` | 文档层面的架构概念定义，非独立代码模块。公式：[Execution] + [0-N Enhancement] + [Optional Guarantee] |
| 3 | [x] 已验证 | Ralph 验证循环（7 项检查 + 5 分钟证据时效） | `src/features/verification/index.ts:27-87` (STANDARD_CHECKS: BUILD/TEST/LINT/FUNCTIONALITY/ARCHITECT/TODO/ERROR_FREE), 同文件 `:277-282` (5分钟时效判断), `src/hooks/ralph/verifier.ts` (Architect 审批循环, 最多3次重试) | 7 项检查和 5 分钟时效均有代码实现 |
| 4 | [x] 已验证 | 复杂度分层路由（Haiku/Sonnet/Opus） | `src/features/model-routing/router.ts` (主入口 getModelForTask), `rules.ts` (LOW=Haiku/MEDIUM=Sonnet/HIGH=Opus), `signals.ts` (信号提取), `scorer.ts` (评分器) | 综合词汇信号 + 结构信号 + 规则集做路由决策 |
| 5 | [x] 已验证 | HUD 实时状态栏 | `src/hud/` (32个文件), `src/hud/render.ts` (主渲染), `src/hud/elements/` (16个元素子模块: agents/ralph/todos/context/skills/git/limits 等) | 通过 `settings.json` 的 `statusLine` 字段配置，约 300ms 刷新间隔 |
| 6 | [x] 已验证 | Skill Learning（从会话提取可复用模式） | `src/hooks/learner/` (13个文件): `auto-learner.ts` (自动检测), `detector.ts` (触发检测), `writer.ts` (写入), `validator.ts` (质量验证), `promotion.ts` (Project→User 升级) | 存储位置：Project 级 `.omc/skills/` / User 级 `~/.claude/skills/omc-learned/` |
| 7 | [!] 有偏差 | ~~31~~ **约 36** 个 Hook 功能模块 | `hooks/hooks.json` (配置文件, 声明11种事件), `src/hooks/` (约36个子模块目录: keyword-detector/ralph/learner/autopilot/ultrawork/persistent-mode/code-simplifier 等) | "31"来自 `docs/ARCHITECTURE.md` 的描述，实际 `src/hooks/` 下约 36 个模块目录 |

## GhostCode 的差异化定位

GhostCode 不是简单的功能叠加，而是在三个项目的基础上做出的架构创新：

1. **Rust 核心** - 相比 Python(CCCC)/Go(ccg)/TS(omc)，Rust 提供更好的性能和内存安全
2. **统一命令注册表** - 将 Slash/MCP/Keywords 抽象为一套 Action Unit
3. **协议驱动** - 所有集成通过标准 MCP/JSON-RPC 接口，不依赖内部实现
4. **单二进制分发** - 一个 Rust 二进制 + 一个 TS Plugin 壳，安装极简

---

## 源码验证状态

| 状态 | 含义 |
|------|------|
| [x] 已验证 | 已从源码中确认，附带具体文件路径 |
| [!] 有偏差 | 源码实现与文档描述存在差异，已修正 |

### 验证汇总（2026-02-28 完成）

| 项目 | 总条目 | [x] 已验证 | [!] 有偏差 |
|------|--------|-----------|-----------|
| CCCC | 7 | 6 | 1 (MCP 工具数量: 31非49) |
| ccg-workflow | 8 | 7 | 1 (代码主权: 提示词约定非代码强制) |
| oh-my-claudecode | 8 | 4 | 4 |
| **合计** | **23** | **17 (74%)** | **6 (26%)** |

### 关键偏差修正

| 原描述 | 修正后 | 影响 |
|--------|--------|------|
| CCCC 49 个 MCP 工具 | 实际 **31** 个 | 命名空间设计仍可参考 |
| ccg-workflow Claude 独占写入权(代码强制) | 实际是**提示词层面约定** | GhostCode 需考虑是否做代码级沙箱 |
| omc 三层 Skill 架构(代码实现) | 实际是**文档层面架构概念** | GhostCode 可参考概念但需自行设计代码结构 |
| omc 31 个 Hooks | 实际约 **36** 个模块目录 | 功能比描述更丰富 |

> 注：omc 的「成本追踪」和「showCost」功能 GhostCode 不需要，已从精华列表中移除。

### 深度验证文档

以下文档包含 GhostCode 核心模块的源码级实现细节（可直接作为 Rust 重实现参考）：

| 文档 | 对应 GhostCode 模块 | 参考项目 |
|------|---------------------|---------|
| `docs/ref-ralph-engine.md` | 验证引擎 Ralph 循环 | oh-my-claudecode |
| `docs/ref-comm-kernel.md` | 通信内核 + Agent 管理 | CCCC |
| `docs/ref-model-router.md` | 多模型路由 + codeagent-wrapper | ccg-workflow |
