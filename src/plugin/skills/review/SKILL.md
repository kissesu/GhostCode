---
name: review
description: 双模型交叉审查：并行审查代码变更，分级处理 Critical/Warning/Info
aliases:
  - team review
  - 团队审查
do_not_use_when: 没有代码变更、或变更太小不值得双模型审查
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

- 用户输入 "team review"、"ccg:team-review"、"团队审查"
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

## Steps

### Step 0: MANDATORY Prompt 增强

确认审查的范围：全量 git diff、特定文件列表、或特定提交范围。
读取 `.claude/team-plan/` 下对应计划文件（若存在）作为审查基准。

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

### Step 1.5: 开启 Session Gate（MANDATORY，不可跳过）

调用 ghostcode_session_gate_open(command_type="review", required_models=["codex", "gemini"])
→ 得到 session_id，后续所有 submit/close 都使用此 session_id
若 Daemon 离线 → 终止流程，报告错误「GhostCode Daemon 未运行」

### Step 2: 多模型并行审查（PARALLEL）

**CRITICAL**: 必须在一条消息中同时发起两个 Bash 后台调用，run_in_background: true。

**Bash 调用 1（Codex 后端/Rust 审查）**：
```bash
~/.ghostcode/bin/ghostcode-wrapper --backend codex --workdir "$(pwd)" --group-id "$GHOSTCODE_GROUP_ID" --timeout 600 --stdin <<'CODEX_REVIEW'
ROLE_FILE: ~/.ghostcode/prompts/codex-reviewer.md

你正在对 GhostCode 项目（Rust 核心 + TS Plugin 多 Agent 协作平台）的代码变更进行后端审查。

请重点审查 Rust 核心代码（src/core/src/ 相关文件），维度包括：
1. 逻辑正确性：业务逻辑错误、边界条件处理、数据流正确性
2. 并发安全性：竞态条件、死锁风险、tokio 任务取消安全
3. 内存安全性：所有权规则遵守、unsafe 块使用合规性、生命周期正确性
4. 错误处理：错误传播完整性、panic 使用场景、Result 处理覆盖率

输出 JSON findings：
{
  "findings": [
    {
      "severity": "Critical|Warning|Info",
      "dimension": "logic|security|memory-safety|concurrency|error-handling",
      "file": "文件路径",
      "line": 行号,
      "description": "问题描述",
      "fix_suggestion": "修复建议"
    }
  ]
}
CODEX_REVIEW
```

**Bash 调用 2（Gemini 前端/TS Plugin 审查）**：
```bash
~/.ghostcode/bin/ghostcode-wrapper --backend gemini --workdir "$(pwd)" --group-id "$GHOSTCODE_GROUP_ID" --timeout 600 --stdin <<'GEMINI_REVIEW'
ROLE_FILE: ~/.ghostcode/prompts/gemini-reviewer.md

你正在对 GhostCode 项目（Rust 核心 + TS Plugin 多 Agent 协作平台）的代码变更进行前端审查。

请重点审查 TS Plugin 代码（src/plugin/src/ 相关文件），维度包括：
1. 可维护性：代码可读性、函数复杂度、命名规范、注释质量
2. React 最佳实践：组件设计、状态管理、副作用处理、性能优化
3. 类型安全：类型推断正确性、类型断言使用、any 类型滥用
4. IPC 协议合规：消息格式是否符合 Rust 核心约定、错误码处理、版本兼容性
5. hook 生命周期：hook 调用顺序正确性、副作用清理、依赖数组完整性

输出 JSON findings：
{
  "findings": [
    {
      "severity": "Critical|Warning|Info",
      "dimension": "patterns|maintainability|type-safety|ipc-compliance|hook-lifecycle",
      "file": "文件路径",
      "line": 行号,
      "description": "问题描述",
      "fix_suggestion": "修复建议"
    }
  ]
}
GEMINI_REVIEW
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
                               output_type="review_findings",
                               data=<codex wrapper 输出>)
```

Gemini wrapper 完成后，调用：
```
ghostcode_session_gate_submit(session_id=<session_id>,
                               model="gemini",
                               output_type="review_findings",
                               data=<gemini wrapper 输出>)
```

### Step 3: 综合发现

调用 ghostcode_session_gate_close(session_id=<session_id>)
→ 返回合并输出（含 partial 标记和 missing_models 列表）
→ 若 SESSION_INCOMPLETE → 终止，必须补全 missing_models
→ 若 partial=true → 报告顶部标注 WARNING PARTIAL_SESSION（有模型使用了 bypass）

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

### Step 5: 决策门

**Critical > 0**：
- 展示发现，使用 AskUserQuestion 询问：「立即修复 / 跳过」
- 选择修复 → Lead 直接修复（Rust 问题参考 Claude 建议，TS 问题参考 Gemini 建议）
- 修复后重新运行受影响的审查维度
- 重复直到 Critical = 0

**Critical = 0**：
- 报告通过，建议提交代码
- 提示：`git add -A && git commit -m "<type>: <描述>"`

### Step 6: 上下文检查点

报告当前上下文使用量。

## Exit Criteria

- [ ] Claude + Gemini 审查均完成
- [ ] 所有发现已综合分级（Critical/Warning/Info）
- [ ] Critical = 0（已修复或用户明确跳过）
- [ ] 审查报告已输出
- [ ] GhostCode 特定：Rust unsafe 块已逐一审查
