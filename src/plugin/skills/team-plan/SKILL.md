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

**CRITICAL**: 必须在一条消息中同时发起两个后台调用，run_in_background: true。

Agent 1（Claude/Codex 路由 - Rust 核心分析）：
- 技术可行性评估（Rust 编译器约束、tokio 异步限制）
- 推荐实施方案（精确到文件和函数）
- Rust 模块依赖分析
- 风险评估

Agent 2（Gemini 路由 - TS Plugin 分析）：
- TS Plugin 可行性评估（hook 生命周期、skill 格式）
- 推荐实施方案（精确到文件和函数）
- Plugin 与核心 IPC 接口对齐方案
- 集成风险

等待两个 Agent 完成（timeout: 600000ms），不可提前终止。

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
