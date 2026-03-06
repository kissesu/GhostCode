---
name: team-plan
description: 多模型协作规划：并行分析 → 消除歧义 → 零决策并行实施计划
aliases:
  - team plan
  - 团队规划
do_not_use_when: 无约束集文档、或任务过于简单不需要并行拆分
---

## Purpose

基于 team-research 产出的约束集，通过多模型并行分析生成零决策的并行实施计划。

计划的核心价值：让 Builder teammates 能无决策机械执行。每个子任务的文件范围必须隔离，
确保并行实施不冲突。GhostCode 的特殊性是 Rust 核心（src/core/）和 TS Plugin
（src/plugin/）可能需要协调变更，这时用依赖关系（DEP-N）而非文件冲突来处理。

产出：`.claude/team-plan/<任务名>.md` 计划文件，包含分层并行任务清单。

## Use When

- 用户输入 "team plan"、"ccg:team-plan"、"团队规划"
- 已有 team-research 产出的约束集文档
- 需要将需求拆分为可并行执行的子任务
- 需要生成 Builder 可无决策执行的计划文件

## Do Not Use When

- 尚未完成 team-research（约束集不存在时禁止跳过直接规划）
- 单人单文件的简单修改（直接 spec-impl 更高效）
- 任务无法拆分为独立子任务

## Guardrails

- 多模型分析是 **mandatory**：必须同时调用 Claude（后端）和 Gemini（前端）
- 不写产品代码，只做分析和规划
- 计划文件必须包含多模型的实际分析摘要
- 每个子任务文件范围不重叠（硬性规则，不可违反）
- 无法避免重叠时，必须设为依赖关系而非并行
- 使用 AskUserQuestion 解决任何歧义
- GhostCode 特定：Rust 变更和 TS Plugin 变更必须作为不同 Layer 处理

## Steps

### Step 0: MANDATORY Prompt 增强

分析 $ARGUMENTS，确认规划目标和范围边界。
读取 `.claude/team-plan/<任务名>-research.md` 约束集文档，若不存在则提示先运行 team-research。

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

### Step 2: 多模型并行分析（PARALLEL）

**CRITICAL**: 必须在一条消息中同时发起两个 Bash 后台调用，run_in_background: true。

**Bash 调用 1（Codex 后端分析）**：
```bash
~/.ghostcode/bin/ghostcode-wrapper --backend codex --workdir "$(pwd)" --timeout 600 --stdin <<'CODEX_TASK'
ROLE_FILE: ~/.ghostcode/prompts/codex-analyzer.md

你正在为 GhostCode 项目（Rust 核心 + TS Plugin 多 Agent 协作平台）进行 Rust 核心分析规划。

请基于 team-research 产出的约束集，提供：
1. 技术可行性评估：Rust 编译器约束、tokio 异步限制、内存安全要求
2. 推荐实施方案：精确到文件路径和函数名的 Rust 核心实施步骤
3. Rust 模块依赖分析：哪些模块需要先实现、模块间接口约定
4. 风险评估：编译时间风险、并发安全风险、接口兼容性风险及缓解方案

请以结构化 JSON 格式输出，包含 feasibility、implementation_plan、module_dependencies、risks 字段。
CODEX_TASK
```

**Bash 调用 2（Gemini 前端分析）**：
```bash
~/.ghostcode/bin/ghostcode-wrapper --backend gemini --workdir "$(pwd)" --timeout 600 --stdin <<'GEMINI_TASK'
ROLE_FILE: ~/.ghostcode/prompts/gemini-analyzer.md

你正在为 GhostCode 项目（Rust 核心 + TS Plugin 多 Agent 协作平台）进行 TS Plugin 分析规划。

请基于 team-research 产出的约束集，提供：
1. TS Plugin 可行性评估：hook 生命周期约束、skill 格式限制、TypeScript 类型安全要求
2. 推荐实施方案：精确到文件路径和函数名的 TS Plugin 实施步骤
3. Plugin 与核心 IPC 接口对齐方案：消息格式约定、错误处理对齐、版本兼容策略
4. 集成风险：IPC 协议变更风险、hook 调用时序风险、类型不匹配风险及缓解方案

请以结构化 JSON 格式输出，包含 feasibility、implementation_plan、ipc_alignment、risks 字段。
GEMINI_TASK
```

等待两个后台任务完成：使用 TaskOutput(block: true, timeout: 600000) 读取各自结果。

**失败处理**：若 wrapper 退出码非 0（如 CLI 不可用退出码 127），log 错误并继续执行（用 Claude 自身分析替代），不终止整个流程。

### Step 3: 综合分析 + 任务拆分

后端方案以 Claude/Codex 为准，前端/Plugin 方案以 Gemini 为准。

拆分原则：
- 每个子任务文件范围不重叠（**强制**）
- 如果无法避免重叠 → 设为依赖关系
- 每个子任务有具体实施步骤和验收标准
- GhostCode 特定：Rust 核心任务先于 TS Plugin 任务（因为 Plugin 调用核心接口）

按依赖关系分层（Layer）：
- **Layer 1**：无依赖的并行任务
- **Layer 2**：依赖 Layer 1 完成的任务
- **Layer N**：依此类推

### Step 4: 写入计划文件

路径：`.claude/team-plan/<任务名>.md`

```markdown
# Team Plan: <任务名>

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

### Step 5: 用户确认

展示计划摘要（子任务数、并行分组、Builder 数量）。
使用 AskUserQuestion 请求确认。
确认后提示：`计划已就绪，运行 /team-exec 开始并行实施`

### Step 6: 上下文检查点

报告当前上下文使用量。
如果接近 80K：建议 `/clear` 后运行 `/team-exec`。

## Exit Criteria

- [ ] 多模型分析完成（Claude + Gemini 均输出结果）
- [ ] 子任务文件范围无冲突（或已通过依赖解决）
- [ ] TDD 验收标准已为每个子任务定义
- [ ] 计划文件已写入 `.claude/team-plan/`
- [ ] 用户已确认计划
- [ ] 并行分组（Layer 结构）清晰无歧义
