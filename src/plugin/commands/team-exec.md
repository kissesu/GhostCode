---
description: 'Agent Teams 并行实施：按计划的 TDD 排序忠实执行，spawn Builder teammates 并行写代码'
---

## Purpose

读取 team-plan 产出的计划文件，按 Layer 分组 spawn Builder agents 并行实施，
Lead 只做协调，不写产品代码。

实施是纯机械执行——所有决策已在 team-plan 阶段完成。Builder teammates 按文件范围
严格隔离并行工作，Lead 负责监控进度、协调阻塞、汇总报告。

GhostCode 特殊性：Rust 核心的 Builder 需要运行 `cargo build + cargo test`，
TS Plugin 的 Builder 需要运行 `pnpm typecheck + pnpm test`。

## Use When

- 用户输入 "team exec"、"ccg:team-exec"、"团队执行"
- 已有 team-plan 产出的计划文件（`.claude/team-plan/*.md`）
- 需要并行实施多个独立子任务
- Agent Teams 功能已启用（CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1）

## Do Not Use When

- 没有 team-plan 计划文件（先运行 /team-plan）
- Agent Teams 未启用（无法 spawn Builder teammates）
- 任务只有一个子任务（单 Builder 无并行收益）
- 子任务之间存在未解决的文件冲突

## Guardrails

- **前置条件**：`.claude/team-plan/` 下必须有计划文件，没有则终止
- **Agent Teams 必须启用**：需要 `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`
- Lead 绝不直接修改产品代码
- 每个 Builder 只能修改分配给它的文件（硬性约束）
- GhostCode 特定：Rust Builder 不得修改 TS Plugin 文件，反之亦然
- Builder 失败不影响其他 Builder 继续执行

## GhostCode Daemon 集成（MANDATORY）

执行任何步骤之前，必须先初始化 GhostCode MCP 工具：

1. 调用 ToolSearch 搜索 "+ghostcode message" 加载 GhostCode MCP 工具
2. 调用 ghostcode_group_info 确认 Daemon 在线且 Group 存在
   - 如果失败：立即报错「GhostCode Daemon 未运行，请先启动 Daemon」，终止流程
3. 后续步骤中使用以下 MCP 工具发送进度消息（Dashboard 实时可见）：
   - ghostcode_message_send: 发送消息
   - ghostcode_dashboard_snapshot: 获取当前状态快照
   - ghostcode_actor_list: 查看活跃 Actor 列表

## Steps

### Step 0: MANDATORY Prompt 增强

确认使用的计划文件路径。如果 $ARGUMENTS 指定了任务名，优先使用对应计划文件。
否则使用 `.claude/team-plan/` 下最新的计划文件。

### Step 1: 前置检查

检测 Agent Teams 是否可用：

```bash
# 检查环境变量
echo $CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS
```

若不可用，输出启用指引后终止：
```
Agent Teams 未启用。请先在 settings.json 中添加：
{ "env": { "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" } }
```

读取 `.claude/team-plan/` 下计划文件。若无计划文件，提示先运行 `/team-plan <任务描述>`，终止。

**MCP 调用**：前置检查通过后，发送启动消息：
```
ghostcode_message_send({ text: "team-exec 启动：准备解析计划文件" })
```

### Step 2: 解析计划

解析子任务列表、文件范围、依赖关系、并行分组（Layer 结构）。

向用户展示摘要并确认：
```
即将并行实施：
- 子任务：N 个
- 并行分组：Layer 1 (X 个并行) → Layer 2 (Y 个)
- Builder 数量：N 个
确认开始？
```

**MCP 调用**：用户确认后发送消息：
```
ghostcode_message_send({ text: "team-exec 确认启动：N 个子任务，M 个 Builder，分 L 层并行" })
```

### Step 3: 创建 Team + Spawn Builders

创建 Agent Team，按 Layer 分组 spawn Builder teammates（Sonnet）。

每个 Builder 的 spawn prompt 必须包含：

```
你是 Builder，负责实施一个子任务。严格按照以下指令执行。

## 你的任务
<从计划文件中提取该 Builder 负责的子任务全部内容，包括实施步骤>

## 工作目录
<绝对路径>

## 文件范围约束（硬性规则）
你只能创建或修改以下文件：
<文件列表>
严禁修改任何其他文件。违反此规则等于任务失败。

## GhostCode 验收规范
- Rust 文件：cargo build 零警告 + cargo test 全通过
- TS 文件：pnpm typecheck + pnpm test 全通过
- 遵循中文注释规范，作者署名 Atlas.oi

## TDD 要求
1. 先创建测试文件（xxx_test.rs 或 xxx.test.ts）
2. 运行测试确认 Red（测试失败）
3. 写实现代码让测试通过（Green）
4. 重构（Refactor）

## GhostCode 消息报告（MANDATORY）
使用 ToolSearch 搜索 "+ghostcode message" 加载 GhostCode MCP 工具，然后在关键节点发送消息：
- 开始任务：ghostcode_message_send({ text: "开始: <任务名>", to: ["lead"] })
- 完成任务：ghostcode_message_send({ text: "完成: <任务名>，文件: <列表>", to: ["lead"] })
- 遇到阻塞：ghostcode_message_send({ text: "阻塞: <描述>", to: ["lead"], priority: "attention" })
MCP 工具调用失败时报告错误，不静默忽略。

完成所有步骤后，使用 TaskUpdate 将任务标记为 completed。
```

依赖关系：Layer 2 的 Builder 任务设为依赖 Layer 1 的对应任务，等 Layer 1 完成后自动解锁。
Spawn 完成后，进入 delegate 模式，只协调不写码。

### Step 4: 监控进度

等待所有 Builder 完成。

**MCP 调用**：定期调用 `ghostcode_dashboard_snapshot` 展示 Dashboard 实时状态（Actor 列表、消息流）。

如果某个 Builder 遇到问题并发消息求助：
- 分析问题，给出指导建议
- 不要自己写代码替它完成

如果某个 Builder 失败：
- 记录失败原因
- 不影响其他 Builder 继续执行

### Step 5: 汇总 + 清理

所有 Builder 完成后，汇总报告：

```markdown
## Team 并行实施完成

### 变更摘要
| Builder | 子任务 | 状态 | 修改文件 |
|---------|--------|------|----------|
| Builder 1 | <名称> | 完成/失败 | file1, file2 |
| Builder 2 | <名称> | 完成/失败 | file3, file4 |

### 后续建议
1. 运行完整测试验证集成：cargo test + pnpm test
2. 检查 Rust 核心与 TS Plugin 的 IPC 接口是否对齐
3. 运行 team-review 进行多模型审查
```

**MCP 调用**：汇总完成后发送消息：
```
ghostcode_message_send({ text: "team-exec 完成：N/M 个子任务成功，详见变更摘要" })
```

关闭所有 teammates，清理 team。

## Exit Criteria

- [ ] Agent Teams 前置条件已验证
- [ ] 计划文件已读取并解析
- [ ] 所有 Builder 任务完成（或明确失败并记录原因）
- [ ] Rust 任务：cargo build 零警告 + cargo test 全通过
- [ ] TS 任务：pnpm typecheck + pnpm test 全通过
- [ ] 变更摘要已输出
- [ ] Team 已清理
