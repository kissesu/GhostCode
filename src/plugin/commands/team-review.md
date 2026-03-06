---
description: '双模型交叉审查：并行审查代码变更，分级处理 Critical/Warning/Info'
---

## Purpose

对 team-exec 产出的代码变更进行双模型独立交叉审查，捕获单模型审查遗漏的盲区，
产出分级审查报告（Critical/Warning/Info），并强制修复所有 Critical 问题。

双模型交叉验证的价值：Claude 擅长后端逻辑/安全审查，Gemini 擅长前端模式/可维护性审查，
两者盲区不重叠，合并后覆盖面更广。

GhostCode 特殊性：
- Rust 核心代码审查重点：内存安全、并发安全、错误传播
- TS Plugin 审查重点：hook 生命周期正确性、IPC 协议合规、类型安全

## Use When

- 用户输入 "team review"、"gc:team-review"、"团队审查"
- 代码实施完成后（team-exec 或 spec-impl 之后）
- 需要多视角的代码质量评估
- 准备合并/发布前的质量门控

## Do Not Use When

- 没有任何代码变更（git diff 为空）
- 仅修改文档/注释（可跳过双模型审查）
- 处于 Red 阶段（测试有意失败的 TDD 阶段）

## Guardrails

- **MANDATORY**: Claude 和 Gemini 必须都完成审查后才能综合（不可只用一个）
- 审查范围限于 git diff 的变更，不做范围蔓延
- Critical 问题必须修复后才能结束，不可忽略
- Lead 可以直接修复 Critical 问题（审查阶段允许写代码）
- GhostCode 特定：Rust unsafe 代码块必须列为 Critical 级别审查

## GhostCode Daemon 集成（MANDATORY）

执行任何步骤之前，必须先初始化 GhostCode MCP 工具：

1. 调用 ToolSearch 搜索 "+ghostcode message" 加载 GhostCode MCP 工具
2. 调用 ghostcode_group_info 确认 Daemon 在线且 Group 存在
   - 如果失败：立即报错「GhostCode Daemon 未运行，请先启动 Daemon」，终止流程
3. 后续步骤中使用以下 MCP 工具发送进度消息（Dashboard 实时可见）：
   - ghostcode_message_send: 发送消息
   - ghostcode_dashboard_snapshot: 获取当前状态快照

## Steps

### Step 0: MANDATORY Prompt 增强

确认审查的范围：全量 git diff、特定文件列表、或特定提交范围。
读取 `.claude/team-plan/` 下对应计划文件（若存在）作为审查基准。

**MCP 调用**：Prompt 增强完成后发送消息：
```
ghostcode_message_send({ text: "team-review 启动：审查范围 <git diff 摘要>" })
```

### Step 1: 收集变更产物

运行 git diff 获取变更摘要：

```bash
# 获取变更文件列表
git diff --name-only HEAD

# 获取统计信息
git diff --stat HEAD

# 获取完整 diff（用于模型审查）
git diff HEAD
```

如果有对应的计划文件，读取约束集和成功判据作为审查基准（OK-N 判据）。
列出所有被修改的文件，分类：Rust 文件 / TS 文件 / 其他。

### Step 2: 多模型并行审查（PARALLEL）

**CRITICAL**: 必须在一条消息中同时发起两个后台 Bash 调用。

Agent 1（Claude/Codex 路由 - 后端/Rust 审查）：
- 审查维度：logic、security、memory-safety、concurrency、error-handling
- Rust 特定：所有权正确性、unsafe 块安全性、异步任务取消安全
- 输出 JSON：{ findings: [{severity, dimension, file, line, description, fix_suggestion}] }

Agent 2（Gemini 路由 - 前端/TS Plugin 审查）：
- 审查维度：patterns、maintainability、type-safety、ipc-compliance、hook-lifecycle
- TS 特定：类型推断正确性、hook 调用顺序、IPC 消息格式合规
- 输出 JSON：{ findings: [{severity, dimension, file, line, description, fix_suggestion}] }

等待两个 Agent 完成（timeout: 600000ms）。

**MCP 调用**：双模型审查完成后发送消息：
```
ghostcode_message_send({ text: "双模型审查完成，开始综合发现" })
```

### Step 3: 综合发现

合并两个模型的发现，去重重叠问题。

按严重性分级：
- **Critical**：安全漏洞、逻辑错误、数据丢失风险、Rust unsafe 误用 → 必须修复
- **Warning**：模式偏离、可维护性问题、类型不精确 → 建议修复
- **Info**：小改进建议、注释补充 → 可选修复

### Step 4: 输出审查报告

**MANDATORY**: 审查报告必须使用表格格式输出，提升可读性和结构化程度。

```markdown
## 审查报告

**审查范围**：N 个文件 | +X / -Y 行
**审查模型**：Rust 后端 Agent + TS/前端 Agent 独立交叉审查

### Critical (X 个) — 必须修复

| # | 维度 | 文件 | 行号 | 问题描述 | 修复建议 |
|---|------|------|------|----------|----------|
| C1 | 安全 | `file.rs` | 42 | 描述 | 建议 |
| C2 | 逻辑 | `handler.ts` | 15 | 描述 | 建议 |

### Warning (Y 个) — 建议修复

| # | 维度 | 文件 | 行号 | 问题描述 | 修复建议 |
|---|------|------|------|----------|----------|
| W1 | 模式 | `utils.ts` | 88 | 描述 | 建议 |

### Info (Z 个) — 可选

| # | 维度 | 文件 | 行号 | 问题描述 | 修复建议 |
|---|------|------|------|----------|----------|
| I1 | 维护 | `helper.rs` | 20 | 描述 | 建议 |

### 已通过检查

| 检查项 | 状态 |
|--------|------|
| 内存安全（Rust 所有权规则） | 通过 |
| IPC 协议合规 | 通过 |
| 错误处理完整性 | 通过 |
```

**MCP 调用**：审查报告输出后发送消息：
```
ghostcode_message_send({ text: "审查报告：Critical=X, Warning=Y, Info=Z" })
```

### Step 5: 决策门

**Critical > 0**：
- 展示发现，使用 AskUserQuestion 询问：「立即修复 / 跳过」
- 选择修复 → Lead 直接修复（Rust 问题参考 Claude 建议，TS 问题参考 Gemini 建议）
- 修复后重新运行受影响的审查维度
- 重复直到 Critical = 0

**Critical = 0**：
- 报告通过，建议提交代码
- 提示：`git add -A && git commit -m "<type>: <描述>"`

**MCP 调用**：决策门通过后发送消息：
```
ghostcode_message_send({ text: "审查通过：Critical=0，建议提交代码" })
```

### Step 6: 上下文检查点

报告当前上下文使用量。

## Exit Criteria

- [ ] Claude + Gemini 审查均完成
- [ ] 所有发现已综合分级（Critical/Warning/Info）
- [ ] Critical = 0（已修复或用户明确跳过）
- [ ] 审查报告已输出
- [ ] GhostCode 特定：Rust unsafe 块已逐一审查
