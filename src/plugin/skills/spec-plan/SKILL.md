---
name: spec-plan
description: 多模型分析 → 消除歧义 → 零决策可执行计划
aliases:
  - spec plan
  - 规范规划
do_not_use_when: spec-research 约束集不存在、或任务过于简单无需多模型分析
---

## Purpose

基于 spec-research 产出的约束集，通过多模型并行分析消除所有实施歧义，产出让
Builder 能够零决策机械执行的详细实施计划。

「零决策」意味着计划精确到文件、函数、行号粒度，Builder 读完后可以直接开始写代码，
不需要再做任何架构或设计决策。

GhostCode 双层架构的 spec-plan 需要同时规划 Rust 核心变更和 TS Plugin 变更，
并明确 IPC 接口的版本化约定。

## Use When

- 用户输入 "spec plan"、"ccg:spec-plan"、"规范规划"
- 已完成 spec-research，约束集文档存在
- 需要生成零决策的详细实施计划
- 存在多条可行路径，需要通过多模型分析选择最优方案

## Do Not Use When

- spec-research 尚未完成（约束集不存在时禁止跳过）
- 任务已有现成的明确方案（直接 spec-impl）
- 任务规模太小，过度规划浪费时间

## Steps

### Step 0: MANDATORY Prompt 增强

确认规划目标。读取 `.claude/spec-changes/<变更名>/research.md` 约束集文档。
若不存在，提示先运行 `/spec-research <需求描述>`，终止。

### Step 1: 读取约束集

完整读取 research.md，提取：
- 所有硬约束（HC-N）——这些约束在规划中不可违反
- 所有软约束（SC-N）——这些约束在规划中尽量遵守
- 所有依赖关系（DEP-N）——决定任务执行顺序
- 成功判据（OK-N）——每个任务的验收标准

### Step 2: 上下文收集

扫描将要变更的模块，了解现有代码结构：

```bash
# 理解现有 Rust 核心接口
grep -n "pub fn\|pub struct\|pub enum\|pub trait" \
  src/core/src/<相关模块>.rs | head -30

# 理解现有 TS Plugin 接口
grep -n "export function\|export interface\|export type" \
  src/plugin/src/<相关模块>.ts | head -30

# 检查现有测试用例结构
find src -name "*_test.rs" -o -name "*.test.ts" | head -10
```

### Step 3: 多模型并行分析（PARALLEL）

**CRITICAL**: 必须在一条消息中同时发起两个后台调用，run_in_background: true。

Agent 1（Claude/Codex 路由 - Rust 核心方案）：
输入：需求 + 约束集 + Rust 核心现有代码结构
输出：
1. 技术可行性评估（基于 HC-N 约束）
2. 推荐实施方案（精确到 file:function 粒度）
3. TDD 测试用例设计（测试先于实现）
4. 风险评估和缓解方案

Agent 2（Gemini 路由 - TS Plugin 方案）：
输入：需求 + 约束集 + TS Plugin 现有代码结构
输出：
1. Plugin 可行性评估（基于 HC-N 约束）
2. 推荐实施方案（精确到 file:function 粒度）
3. TDD 测试用例设计
4. IPC 接口对齐方案

等待两个 Agent 完成（timeout: 600000ms），不可提前终止。

### Step 4: 综合分析 + 矛盾识别

对比两个模型的方案：
- 后端（Rust）方案以 Claude/Codex 为权威
- 前端/Plugin（TS）方案以 Gemini 为权威
- 若存在矛盾（如 IPC 接口定义冲突），提交给用户确认

### Step 5: 用户确认关键决策

对分析中识别的关键决策点，使用 AskUserQuestion 逐一确认：
- 每次只问 1-3 个问题
- 为每个选项说明优劣
- 用户回答后将决策固化为约束

### Step 6: 写入计划文件

路径：`.claude/spec-changes/<变更名>/tasks.md`

```markdown
# Spec Plan: <变更名>

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

### Layer 1（并行）

#### Task 1: <名称>
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

#### Task 2: <名称>
...

### Layer 2（依赖 Layer 1）
...

## 文件冲突检查
无冲突 / 已通过依赖关系解决

## 成功判据映射
- [OK-1] → Task 1 验收
- [OK-2] → Task 2 验收
```

### Step 7: 用户确认计划

展示计划摘要（任务数、Layer 结构、预计时间）。
使用 AskUserQuestion 请求最终确认。
确认后提示：`计划已就绪，运行 /spec-impl <变更名> 开始实施`

### Step 8: 上下文检查点

报告当前上下文使用量。
如果接近 80K：建议 `/clear` 后运行 `/spec-impl`。

## Exit Criteria

- [ ] 多模型分析完成（Claude + Gemini 均输出结果）
- [ ] 所有矛盾已解决（无冲突技术方案）
- [ ] 所有关键决策已通过用户确认
- [ ] TDD 流程（Red/Green/Refactor）已为每个任务定义
- [ ] 计划文件已写入 `.claude/spec-changes/<变更名>/tasks.md`
- [ ] 用户已最终确认计划
