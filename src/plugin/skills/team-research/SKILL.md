---
name: team-research
description: 多 Agent 并行研究：代码库探索 + 约束集提取 + 成功判据定义
aliases:
  - team research
  - 团队研究
do_not_use_when: 任务范围已明确、无需探索代码库、或已有现成约束集文档
---

## Purpose

并行调度多个 Agent 对 GhostCode 代码库进行深度探索，产出约束集和可验证的成功判据。

Research 阶段的产出不是信息堆砌，而是**约束集**。每条约束缩小解决方案空间，告诉后续
的 team-plan 阶段「不要考虑这个方向」，使 plan 阶段能够产出零决策计划，Builder 能够
无决策机械执行。

GhostCode 技术栈特殊性：Rust 核心引擎（src/core/）+ TypeScript Plugin 薄壳
（src/plugin/），两层之间通过 Unix socket / stdio JSON-RPC 通信，探索时需分别对待。

## Use When

- 用户输入 "team research"、"ccg:team-research"、"团队研究"
- 需要对 GhostCode 代码库进行全面分析（Rust 核心 + TS Plugin 双层）
- 需要提取硬约束和软约束
- 需求描述存在歧义，无法直接规划
- 启动一个新功能模块前需要了解现有架构约束

## Do Not Use When

- 任务范围已明确，约束集已存在（直接使用 team-plan）
- 仅修改单一文件的小型改动（过度杀鸡用牛刀）
- 时间紧迫且风险可接受时（可跳过直接 team-plan）

## Guardrails

- **STOP! BEFORE ANY OTHER ACTION**: 必须先做 Step 0 Prompt 增强，不可跳过
- 按上下文边界划分探索范围，不按角色划分（禁止「架构师 Agent」「安全专家 Agent」）
- 多模型协作是 **mandatory**：Claude（Rust 核心/后端边界）+ Gemini（TS Plugin/前端边界）
- 不做架构决策——只发现约束
- 使用 AskUserQuestion 解决任何歧义，绝不假设
- GhostCode 特定：Rust 核心变更必须与 TS Plugin 接口同步检查

## Steps

### Step 0: MANDATORY Prompt 增强（立即执行，不可跳过）

分析 $ARGUMENTS 的意图、缺失信息、隐含假设，补全为结构化需求：
- 明确目标：用户实际想要达成什么
- 技术约束：GhostCode Rust core + TS plugin 的架构限制
- 范围边界：哪些文件/模块在范围内
- 验收标准：怎么算完成

后续所有步骤使用增强后的需求。

### Step 1: GhostCode 代码库评估

使用 Glob/Grep/Read 扫描项目结构：

```bash
# 扫描 Rust 核心结构
find src/core/src -name "*.rs" | head -30

# 扫描 TS Plugin 结构
find src/plugin/src -name "*.ts" | grep -v test | head -30

# 检查 IPC 协议定义
grep -r "JSON-RPC\|socket\|stdio" src/core/src/ --include="*.rs" -l
```

识别：
- Rust 核心现有模块（daemon、ipc、messaging 等）
- TS Plugin 现有 hook handlers、skills、router
- IPC 边界：什么走 Unix socket，什么走 stdio
- 现有模式：错误处理、日志格式、测试风格

### Step 2: 定义探索边界（按上下文划分）

识别自然的上下文边界（不是功能角色）：

| 边界编号 | 范围 | 描述 |
|---------|------|------|
| 边界 A | src/core/src/ | Rust 核心引擎：daemon、ipc、messaging |
| 边界 B | src/plugin/src/ | TS Plugin 薄壳：hooks、router、skills |
| 边界 C | IPC 协议层 | Unix socket 消息格式、JSON-RPC 结构 |

每个边界应自包含，无需跨边界通信。

### Step 3: 多模型并行探索（PARALLEL）

**CRITICAL**: 必须在一条消息中同时发起两个后台 Bash 调用。

Agent 1（Claude/Codex 路由 - 探索边界 A）：
- 分析 Rust 核心架构约束
- 识别 tokio 异步模式、错误类型、接口定义
- 输出 JSON 格式约束集

Agent 2（Gemini 路由 - 探索边界 B）：
- 分析 TS Plugin 架构约束
- 识别 hook 生命周期、skill 格式、router 策略
- 输出 JSON 格式约束集

等待两个 Agent 完成后继续。

### Step 4: 聚合与综合

合并所有探索输出为统一约束集：

- **硬约束**（HC-N）：技术限制、不可违反的模式（如：Rust 所有权规则、IPC 消息格式）
- **软约束**（SC-N）：惯例、偏好、风格指南（如：注释语言、日志前缀）
- **依赖关系**（DEP-N）：影响实施顺序的跨模块关系
- **风险**（RISK-N）：需要缓解的阻碍

### Step 5: 歧义消解

编译优先级排序的开放问题列表，使用 AskUserQuestion 系统性呈现：
- 分组相关问题
- 为每个问题提供上下文
- 在适用时建议默认值

将用户回答转化为额外约束。

### Step 6: 写入研究文件

路径：`.claude/team-plan/<任务名>-research.md`

```markdown
# Team Research: <任务名>

## 增强后的需求
<结构化需求描述>

## 约束集

### 硬约束
- [HC-1] <约束描述> — 来源：<Agent/用户>
- [HC-2] ...

### 软约束
- [SC-1] <约束描述> — 来源：<Agent/用户>

### 依赖关系
- [DEP-1] <模块A> → <模块B>：<原因>

### 风险
- [RISK-1] <风险描述> — 缓解：<策略>

## 成功判据
- [OK-1] <可验证的成功行为>
- [OK-2] ...

## 开放问题（已解决）
- Q1: <问题> → A: <用户回答> → 约束：[HC/SC-N]
```

### Step 7: 上下文检查点

报告当前上下文使用量。
提示：`研究完成，运行 /clear 后执行 /team-plan <任务名> 开始规划`

## Exit Criteria

- [ ] 多模型探索完成（至少 2 个 Agent 输出）
- [ ] Rust 核心边界和 TS Plugin 边界均已探索
- [ ] 所有歧义已通过用户确认解决
- [ ] 约束集（HC/SC/DEP/RISK）已写入研究文件
- [ ] 成功判据（OK-N）已定义
- [ ] 零开放问题残留
