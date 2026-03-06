---
name: spec-research
description: 需求研究：并行探索代码库，产出约束集 + 可验证成功判据
aliases:
  - spec research
  - 规范研究
do_not_use_when: 需求已明确、约束集已存在、或任务过于简单无需深入研究
---

## Purpose

针对特定需求进行深度代码库研究，通过并行探索产出结构化约束集文档，为后续
spec-plan 阶段提供零歧义的输入基础。

Research 产出的是**约束集**，不是信息堆砌。每条约束缩小解决方案空间，告诉后续阶段
「不要考虑这个方向」。GhostCode 是 Rust 核心 + TS Plugin 双层架构，研究时需同时
探索两层的约束，特别关注 IPC 协议边界。

## Use When

- 用户输入 "spec research"、"ccg:spec-research"、"规范研究"
- 需要对特定需求进行深度研究（新模块、重大变更）
- 需求描述模糊、存在多种可能的实现路径
- 需要提取约束集和成功判据供 spec-plan 使用

## Do Not Use When

- 需求已明确到文件/函数粒度（直接用 spec-plan）
- 约束集已存在（检查 `.claude/spec-changes/` 目录）
- 简单的 bug 修复或单文件改动（过度研究浪费时间）

## Steps

### Step 0: MANDATORY Prompt 增强（立即执行，不可跳过）

**立即执行，不可跳过。**

分析 $ARGUMENTS 的意图、缺失信息、隐含假设，补全为结构化需求：
- **目标**：用户实际想要达成什么业务/技术目标
- **技术约束**：GhostCode Rust core + TS Plugin 的架构限制
- **范围边界**：哪些文件/模块在范围内，哪些明确排除
- **验收标准**：怎么算完成，可观测的成功行为

后续所有步骤使用增强后的需求。

### Step 1: 初始代码库评估

使用 Glob/Grep/Read 扫描相关模块：

```bash
# 扫描 Rust 核心结构
find src/core/src -name "*.rs" -not -path "*/tests/*" | sort

# 扫描 TS Plugin 结构
find src/plugin/src -name "*.ts" -not -path "*/__tests__/*" | sort

# 检查 IPC 协议边界
grep -r "Command\|Response\|Event" src/core/src/ --include="*.rs" -l
```

判断项目规模，确定需要探索的边界范围。

### Step 2: 定义探索边界（按上下文划分，非角色划分）

识别自然的上下文边界：

| 边界 | 范围 | 说明 |
|------|------|------|
| 边界 A | Rust 核心 | src/core/src/ 相关模块 |
| 边界 B | TS Plugin | src/plugin/src/ 相关模块 |
| 边界 C | IPC 协议 | 消息格式、错误码、序列化 |

每个边界应自包含，并行探索无需跨边界通信。

### Step 3: 并行多模型探索

**CRITICAL**: 必须在一条消息中同时发起两个 Bash 后台调用，run_in_background: true。

**Bash 调用 1（Codex 后端分析）**：
```bash
~/.ghostcode/bin/ghostcode-wrapper --backend codex --workdir "$(pwd)" --timeout 600 --stdin <<'CODEX_TASK'
ROLE_FILE: ~/.ghostcode/prompts/codex-analyzer.md

你正在分析 GhostCode 项目（Rust 核心 + TS Plugin 多 Agent 协作平台）的 Rust 核心边界约束。

请探索 src/core/src/ 目录，重点分析：
1. 内存安全约束：所有权规则、借用检查、生命周期约束
2. 并发约束：tokio 异步模式、任务调度、共享状态处理
3. 错误处理约束：错误类型定义、传播方式、Result/Option 使用规范
4. IPC 协议边界：Unix socket/stdio 消息格式、序列化约束

输出 JSON 格式约束集：
{
  "module_name": "Rust 核心边界",
  "existing_structures": ["发现的关键数据结构/函数"],
  "existing_conventions": ["使用中的规范/模式"],
  "constraints_discovered": ["限制解决方案空间的硬约束"],
  "open_questions": ["需要用户确认的歧义"],
  "dependencies": ["跨模块依赖关系"],
  "risks": ["潜在阻碍"],
  "success_criteria_hints": ["可观测的成功行为"]
}
CODEX_TASK
```

**Bash 调用 2（Gemini 前端分析）**：
```bash
~/.ghostcode/bin/ghostcode-wrapper --backend gemini --workdir "$(pwd)" --timeout 600 --stdin <<'GEMINI_TASK'
ROLE_FILE: ~/.ghostcode/prompts/gemini-analyzer.md

你正在分析 GhostCode 项目（Rust 核心 + TS Plugin 多 Agent 协作平台）的 TS Plugin 边界约束。

请探索 src/plugin/src/ 目录，重点分析：
1. 类型安全约束：TypeScript 类型定义、接口规范、类型推断限制
2. hook 生命周期约束：Claude Code Plugin hook 的调用顺序、生命周期、副作用限制
3. skill 格式约束：SKILL.md 格式规范、参数传递方式、输出格式要求
4. IPC 协议对齐：与 Rust 核心的消息格式对齐、JSON-RPC 结构约束

输出 JSON 格式约束集：
{
  "module_name": "TS Plugin 边界",
  "existing_structures": ["发现的关键数据结构/函数"],
  "existing_conventions": ["使用中的规范/模式"],
  "constraints_discovered": ["限制解决方案空间的硬约束"],
  "open_questions": ["需要用户确认的歧义"],
  "dependencies": ["跨模块依赖关系"],
  "risks": ["潜在阻碍"],
  "success_criteria_hints": ["可观测的成功行为"]
}
GEMINI_TASK
```

等待两个后台任务完成：使用 TaskOutput(block: true, timeout: 600000) 读取各自结果。

**失败处理**：若 wrapper 退出码非 0（如 CLI 不可用退出码 127），log 错误并继续执行（用 Claude 自身分析替代），不终止整个流程。

### Step 4: 聚合与综合

收集所有 Agent 输出，合并为统一约束集：

- **硬约束**（HC-N）：技术限制，不可违反的模式
  - 例：`[HC-1] Rust 异步函数必须使用 tokio::spawn，不可使用 std::thread — 来源：Agent A`
- **软约束**（SC-N）：惯例、偏好、风格指南
  - 例：`[SC-1] 所有注释使用中文，作者署名 Atlas.oi — 来源：CLAUDE.md`
- **依赖关系**（DEP-N）：影响实施顺序的跨模块关系
  - 例：`[DEP-1] TS Plugin → Rust Core：Plugin 调用 Core 的 IPC 接口`
- **风险**（RISK-N）：需要缓解的阻碍
  - 例：`[RISK-1] Rust 编译时间长，CI 超时风险 — 缓解：增量编译 + 缓存`

### Step 5: 歧义消解

编译优先级排序的开放问题列表，使用 AskUserQuestion 系统性呈现：
- 分组相关问题，每次呈现不超过 3 个
- 为每个问题提供上下文和建议默认值
- 将用户回答转化为额外约束（HC-N 或 SC-N）

### Step 6: 写入约束集文档

路径：`.claude/spec-changes/<变更名>/research.md`

```markdown
# Spec Research: <变更名>

## 增强后的需求
<结构化需求描述>

## 约束集

### 硬约束
- [HC-1] <约束描述> — 来源：<Agent/用户>

### 软约束
- [SC-1] <约束描述> — 来源：<CLAUDE.md/用户>

### 依赖关系
- [DEP-1] <模块A> → <模块B>：<原因>

### 风险
- [RISK-1] <风险描述> — 缓解：<策略>

## 成功判据
- [OK-1] cargo test 全通过（Rust 核心）
- [OK-2] pnpm test 全通过（TS Plugin）
- [OK-3] <业务可观测行为>

## 开放问题（已解决）
- Q1: <问题> → A: <用户回答> → 约束：[HC/SC-N]
```

### Step 7: 上下文检查点

报告当前上下文使用量。
提示：`研究完成，运行 /clear 后执行 /spec-plan <变更名> 开始规划`

## Exit Criteria

- [ ] 并行探索完成（Rust 核心 + TS Plugin 边界均已覆盖）
- [ ] 所有歧义已通过用户确认解决
- [ ] 约束集（HC/SC/DEP/RISK）已写入文档
- [ ] 成功判据（OK-N）已定义
- [ ] 零开放问题残留
