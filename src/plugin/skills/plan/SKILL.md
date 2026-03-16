---
name: plan
description: '多模型协作规划：规模评估 -> 并行分析 -> 零决策实施计划（合并 spec-plan + team-plan）'
aliases:
  - gc plan
  - 规划
do_not_use_when: 尚未完成 gc:research、或任务过于简单不需要规划
---

## Purpose

统一的规划命令，合并原 spec-plan（单人串行）和 team-plan（多 Builder 并行）的能力。

基于 gc:research 产出的约束集，通过多模型并行分析生成零决策实施计划。
在 Step 0 自动评估任务规模，决定执行模式：
- **parallel**：大型任务，多文件/多模块，生成 Builder 并行任务矩阵（Layer 分层）
- **serial**：小型任务，单模块，生成精确到行号的串行任务列表

产出：`.claude/plan/<任务名>.md` 计划文件，包含 `execution_mode` 字段供 gc:execute 读取。

## Use When

- 用户输入 "gc plan"、"gc:plan"、"plan"、"规划"
- 已完成 gc:research，约束集文档存在
- 需要生成零决策的详细实施计划
- 需要将需求拆分为可执行的子任务

## Do Not Use When

- 尚未完成 gc:research（约束集不存在时禁止跳过直接规划）
- 任务已有现成的明确方案（直接实施更高效）
- 任务极其简单，过度规划浪费时间

## Guardrails

- 多模型分析是 **mandatory**：必须同时调用 Codex（后端）和 Gemini（前端）
- 不写产品代码，只做分析和规划
- 计划文件必须包含多模型的实际分析摘要
- parallel 模式：每个子任务文件范围不重叠（硬性规则，不可违反）
- 无法避免重叠时，必须设为依赖关系而非并行
- 使用 AskUserQuestion 解决任何歧义
- GhostCode 特定：Rust 变更和 TS Plugin 变更必须作为不同 Layer 处理
- 计划文件必须包含 `execution_mode: parallel | serial` 字段

## GhostCode Daemon 集成（MANDATORY）

执行任何步骤之前，必须先初始化 GhostCode MCP 工具：

1. 调用 ToolSearch 搜索 "+ghostcode message" 加载 GhostCode MCP 工具
2. 调用 ghostcode_group_info 确认 Daemon 在线且 Group 存在
   - 如果失败：立即报错「GhostCode Daemon 未运行，请先启动 Daemon」，终止流程
3. 后续步骤中使用以下 MCP 工具发送进度消息（Dashboard 实时可见）：
   - ghostcode_message_send: 发送消息
   - ghostcode_dashboard_snapshot: 获取当前状态快照

## Steps

### Step 0: 规模评估

分析 $ARGUMENTS，确认规划目标和范围边界。

读取 `.claude/research/<任务名>-research.md` 约束集文档，若不存在则提示先运行 gc:research，终止。

**规模判断标准**：
- **parallel**（大型任务）：涉及多文件、多模块、需要多 Builder 并行、Rust + TS 协调变更
- **serial**（小型任务）：单模块、单人串行 TDD、变更范围集中

将判断结果记为 `execution_mode`，后续步骤根据此值分流。

**MCP 调用**：规模评估完成后发送消息：
```
ghostcode_message_send({ text: "gc:plan 启动（execution_mode: <parallel|serial>）：基于约束集 <文件名>" })
```

### Step 1: 上下文收集

用 Glob/Grep/Read 分析项目结构：

```bash
# 了解现有模块结构
ls src/core/src/
ls src/plugin/src/

# 检查现有代码模式
grep -r "pub fn\|pub struct\|pub enum" src/core/src/ --include="*.rs" | head -20
grep -r "export function\|export class\|export const" src/plugin/src/ --include="*.ts" | head -20
```

整理出：技术栈、目录结构、关键文件、现有模式。

### Step 1.5: 开启 Session Gate（MANDATORY，不可跳过）

调用 ghostcode_session_gate_open(command_type="plan", required_models=["codex", "gemini"])
→ 得到 session_id，后续所有 submit/close 都使用此 session_id
若 Daemon 离线 → 终止流程，报告错误「GhostCode Daemon 未运行」

### Step 2: 多模型并行分析（PARALLEL）

**CRITICAL**: 必须在一条消息中同时发起两个 Bash 后台调用，run_in_background: true。

**Bash 调用 1（Codex 后端分析）**：
```bash
~/.ghostcode/bin/ghostcode-wrapper --backend codex --workdir "$(pwd)" --group-id "$GHOSTCODE_GROUP_ID" --timeout 600 --stdin <<'CODEX_TASK'
ROLE_FILE: ~/.ghostcode/prompts/codex-analyzer.md

你正在为 GhostCode 项目（Rust 核心 + TS Plugin 多 Agent 协作平台）进行 Rust 核心分析规划。

请基于约束集文档，提供：
1. 技术可行性评估：Rust 编译器约束、tokio 异步限制、内存安全要求
2. 推荐实施方案：精确到文件路径和函数名的 Rust 核心实施步骤
3. Rust 模块依赖分析：哪些模块需要先实现、模块间接口约定
4. 风险评估：编译时间风险、并发安全风险、接口兼容性风险及缓解方案

请以结构化 JSON 格式输出，包含 feasibility、implementation_plan、module_dependencies、risks 字段。
CODEX_TASK
```

**Bash 调用 2（Gemini 前端分析）**：
```bash
~/.ghostcode/bin/ghostcode-wrapper --backend gemini --workdir "$(pwd)" --group-id "$GHOSTCODE_GROUP_ID" --timeout 600 --stdin <<'GEMINI_TASK'
ROLE_FILE: ~/.ghostcode/prompts/gemini-analyzer.md

你正在为 GhostCode 项目（Rust 核心 + TS Plugin 多 Agent 协作平台）进行 TS Plugin 分析规划。

请基于约束集文档，提供：
1. TS Plugin 可行性评估：hook 生命周期约束、skill 格式限制、TypeScript 类型安全要求
2. 推荐实施方案：精确到文件路径和函数名的 TS Plugin 实施步骤
3. Plugin 与核心 IPC 接口对齐方案：消息格式约定、错误处理对齐、版本兼容策略
4. 集成风险：IPC 协议变更风险、hook 调用时序风险、类型不匹配风险及缓解方案

请以结构化 JSON 格式输出，包含 feasibility、implementation_plan、ipc_alignment、risks 字段。
GEMINI_TASK
```

等待两个后台任务完成：使用 TaskOutput(block: true, timeout: 600000) 读取各自结果。

**wrapper 失败处理（按退出码分级）**：
- exit 127（命令不存在）→ 终止流程，提示「ghostcode-wrapper 未安装，请检查环境」
- exit 124（超时）→ 自动重试一次，仍失败则进入用户确认
- 其他（如 429 额度用完）→ AskUserQuestion 让用户选择：
    [重试] / [跳过并记录 bypass_reason] / [终止整个流程]
  - 用户选「跳过」→ ghostcode_session_gate_submit(session_id, model,
      output_type="bypass", data={}, bypass=true, bypass_reason="quota_exceeded")
  - 用户选「终止」→ ghostcode_session_gate_abort(session_id) 后退出

Codex wrapper 完成后，调用：
```
ghostcode_session_gate_submit(session_id=<session_id>,
                               model="codex",
                               output_type="plan_analysis",
                               data=<codex wrapper 输出>)
```

Gemini wrapper 完成后，调用：
```
ghostcode_session_gate_submit(session_id=<session_id>,
                               model="gemini",
                               output_type="plan_analysis",
                               data=<gemini wrapper 输出>)
```

**MCP 调用**：多模型分析完成后发送消息：
```
ghostcode_message_send({ text: "多模型分析完成，开始综合方案并生成计划" })
```

### Step 3: 综合分析 + 任务拆分

调用 ghostcode_session_gate_close(session_id=<session_id>)
→ 返回合并输出（含 partial 标记和 missing_models 列表）
→ 若 SESSION_INCOMPLETE → 终止，必须补全 missing_models
→ 若 partial=true → 报告顶部标注 WARNING PARTIAL_SESSION（有模型使用了 bypass）

后端方案以 Claude/Codex 为准，前端/Plugin 方案以 Gemini 为准。
若存在矛盾（如 IPC 接口定义冲突），提交给用户确认。

**根据 execution_mode 分流**：

#### parallel 模式（大型任务）

拆分原则：
- 每个子任务文件范围不重叠（**强制**）
- 如果无法避免重叠 -> 设为依赖关系
- 每个子任务有具体实施步骤和验收标准
- GhostCode 特定：Rust 核心任务先于 TS Plugin 任务（因为 Plugin 调用核心接口）

按依赖关系分层（Layer）：
- **Layer 1**：无依赖的并行任务
- **Layer 2**：依赖 Layer 1 完成的任务
- **Layer N**：依此类推

#### serial 模式（小型任务）

拆分原则：
- 任务按执行顺序排列，精确到文件、函数、行号粒度
- 每个任务包含 TDD 流程：Red -> Green -> Refactor
- 提取硬约束（HC-N）、软约束（SC-N）、依赖关系（DEP-N）、成功判据（OK-N）

### Step 4: 写入计划文件

路径：`.claude/plan/<任务名>.md`

#### parallel 模式计划文件格式

```markdown
# Plan: <任务名>

execution_mode: parallel

## 概述
<一句话描述>

## 多模型分析摘要

### Claude/Codex 分析（Rust 核心）
<实际返回的关键内容>

### Gemini 分析（TS Plugin）
<实际返回的关键内容>

## 技术方案
<综合最优方案，含关键技术决策>

## 子任务列表

### Task 1: <名称>
- **类型**: Rust 核心 / TS Plugin / 跨层
- **文件范围**: <精确文件路径列表>
- **依赖**: 无 / Task N
- **实施步骤**:
  1. <具体步骤>
  2. <具体步骤>
- **TDD 验收**:
  - 测试文件：<路径>
  - 通过条件：cargo test / pnpm test 全通过

### Task 2: <名称>
...

## 文件冲突检查
无冲突 / 已通过依赖关系解决

## 并行分组
- Layer 1（并行）: Task 1, Task 2
- Layer 2（依赖 Layer 1）: Task 3
```

#### serial 模式计划文件格式

```markdown
# Plan: <任务名>

execution_mode: serial

## 概述
<一句话描述>

## 多模型分析摘要

### Claude/Codex（Rust 核心）
<实际分析的关键内容摘要>

### Gemini（TS Plugin）
<实际分析的关键内容摘要>

## 技术方案
<综合最优方案，含关键技术决策及理由>

## TDD 实施计划

### Task 1: <名称>
- **类型**: Rust 核心 / TS Plugin
- **文件范围**:
  - 测试文件：`<路径>_test.rs` 或 `<路径>.test.ts`
  - 实现文件：`<精确路径>`
- **TDD 流程**:
  1. Red：创建测试文件，运行确认失败
  2. Green：写最少实现代码，测试通过
  3. Refactor：集成到现有模块
- **验收**:
  - `cargo test <测试名>` 全通过
  - `cargo build` 零警告

### Task 2: <名称>
...

## 成功判据映射
- [OK-1] -> Task 1 验收
- [OK-2] -> Task 2 验收
```

**MCP 调用**：计划文件写入后发送消息：
```
ghostcode_message_send({ text: "计划已生成：<文件路径>，execution_mode: <parallel|serial>，N 个子任务" })
```

### Step 5: 用户确认

展示计划摘要（execution_mode、子任务数、并行分组/串行顺序）。
使用 AskUserQuestion 请求确认。
确认后提示：`计划已就绪，运行 gc:execute 开始实施`

### Step 6: 上下文检查点

报告当前上下文使用量。
如果接近 80K：建议 `/clear` 后运行 `gc:execute`。

## Exit Criteria

- [ ] 规模评估完成，execution_mode 已确定
- [ ] 多模型分析完成（Codex + Gemini 均输出结果）
- [ ] 所有矛盾已解决（无冲突技术方案）
- [ ] TDD 验收标准已为每个任务定义
- [ ] 计划文件已写入 `.claude/plan/<任务名>.md`
- [ ] 计划文件包含 `execution_mode` 字段
- [ ] 用户已确认计划
- [ ] parallel 模式：Layer 结构清晰无歧义，文件范围无冲突
- [ ] serial 模式：任务精确到文件/函数/行号粒度
