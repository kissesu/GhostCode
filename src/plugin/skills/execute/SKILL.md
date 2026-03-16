---
name: execute
description: 智能执行引擎：读取 gc:plan 计划文件，按 execution_mode 自动选择并行（Team Builder）或串行（TDD）执行模式
aliases:
  - gc execute
  - 执行
do_not_use_when: 尚未运行 gc:plan 生成计划文件
---

## Purpose

统一执行入口，读取 gc:plan 产出的计划文件（`.claude/plan/<任务名>.md`）中的
`execution_mode` 字段，自动分叉到两种执行模式：

- **parallel**：Daemon 在线 + Agent Teams 并行实施（原 team-exec）
- **serial**：单 Agent TDD 串行实施 + 归档闭环（原 spec-impl）

两种模式根本不同：parallel 需要 GhostCode Daemon 在线和 Agent Teams 环境，
serial 不依赖 Daemon，单 Agent 独立完成全部 TDD 流程。

## Use When

- 用户输入 "gc execute"、"gc:execute"、"执行"
- 已完成 gc:plan，计划文件存在于 `.claude/plan/` 目录
- 需要按计划执行实施

## Do Not Use When

- gc:plan 尚未运行（计划文件不存在）
- 任务已全部完成

## Steps

### Step 0: 读取计划文件

确认任务名称。读取 `.claude/plan/<任务名>.md`，提取 `execution_mode` 字段。

- 若 `execution_mode: parallel` → 进入 **parallel 分支**（Step P1 ~ P3）
- 若 `execution_mode: serial` → 进入 **serial 分支**（Step S1 ~ S2）
- 若计划文件不存在 → 报错：「计划文件不存在，请先运行 `gc:plan <任务名>` 生成计划」，终止

---

## Parallel 分支（Team Builder 并行执行）

### Step P1: Daemon 健康检查

调用 GhostCode MCP 工具验证 Daemon 在线：

1. 调用 ToolSearch 搜索 "+ghostcode message" 加载 GhostCode MCP 工具
2. 调用 `ghostcode_group_info` 确认 Daemon 在线且 Group 存在
   - 如果失败：报错「GhostCode Daemon 未运行，parallel 模式需要 Daemon，请先启动 Daemon」，终止
3. 检测 Agent Teams 是否可用：
   ```bash
   echo $CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS
   ```
   若不可用，输出启用指引后终止：
   ```
   Agent Teams 未启用。请先在 settings.json 中添加：
   { "env": { "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS": "1" } }
   ```

**MCP 调用**：健康检查通过后发送消息：
```
ghostcode_message_send({ text: "gc:execute parallel 启动：Daemon 在线，准备解析计划文件" })
```

### Step P2: Team Builder 并行执行

解析计划文件中的子任务列表、文件范围、依赖关系、并行分组（Layer 结构）。

向用户展示摘要并确认：
```
即将并行实施：
- 子任务：N 个
- 并行分组：Layer 1 (X 个并行) -> Layer 2 (Y 个)
- Builder 数量：N 个
确认开始？
```

用户确认后，创建 Agent Team，按 Layer 分组 spawn Builder teammates（Sonnet）。

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
Spawn 完成后，Lead 进入 delegate 模式，只协调不写码。

**监控进度**：

等待所有 Builder 完成。定期调用 `ghostcode_dashboard_snapshot` 展示 Dashboard 实时状态。

如果某个 Builder 遇到问题并发消息求助：
- 分析问题，给出指导建议
- 不要自己写代码替它完成

如果某个 Builder 失败：
- 记录失败原因
- 不影响其他 Builder 继续执行

### Step P3: 汇总 + 清理

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
3. 运行 gc:review 进行审查
```

**MCP 调用**：汇总完成后发送消息：
```
ghostcode_message_send({ text: "gc:execute parallel 完成：N/M 个子任务成功，详见变更摘要" })
```

关闭所有 teammates，清理 team。

---

## Serial 分支（TDD 串行执行）

### Step S1: TDD 串行执行

按计划文件的任务列表顺序逐一执行，每个任务遵循 TDD 三阶段：

#### Red 阶段

先创建测试文件，让测试编译通过但断言失败：

**Rust 测试**：
```rust
// src/core/src/<模块>/<功能>_test.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_<功能名>() {
        // Red: 断言预期结果，实现还不存在
        let result = <功能函数>(/* 参数 */);
        assert_eq!(result, <预期值>);
    }
}
```

验证 Red（必须）：
```bash
cargo test <测试名> 2>&1
# 预期：测试失败（编译错误或断言失败均可）
```

**TS 测试**：
```typescript
// src/plugin/src/<模块>/<功能>.test.ts
import { describe, it, expect } from "vitest";

describe("<功能名>", () => {
  it("应该 <预期行为>", () => {
    // Red: 写断言，函数还未实现
    expect(<功能函数>()).toBe(<预期值>);
  });
});
```

验证 Red：
```bash
pnpm vitest run src/plugin/src/<模块>/<功能>.test.ts
# 预期：测试失败
```

#### Green 阶段

写最少实现代码，让测试通过：

**Rust 实现**：
```rust
// src/core/src/<模块>/<功能>.rs
pub fn <功能函数>(/* 参数 */) -> <返回类型> {
    // 最少实现，不过度设计
}
```

验证 Green：
```bash
cargo test <测试名>
# 预期：测试通过
cargo build 2>&1 | grep -E "^error"
# 预期：无编译错误
```

**TS 实现**：
```typescript
// src/plugin/src/<模块>/<功能>.ts
export function <功能函数>(/* 参数 */): <返回类型> {
  // 最少实现
}
```

验证 Green：
```bash
pnpm vitest run src/plugin/src/<模块>/<功能>.test.ts
# 预期：测试通过
```

#### Refactor 阶段

集成到现有模块（mod.rs / index.ts），确保测试仍通过：

```bash
# Rust：集成后验证
cargo test && cargo build

# TS：集成后验证
pnpm test && pnpm typecheck
```

#### 外部模型协作（可选）

对复杂算法或性能敏感代码，可调用外部模型获取参考实现。

**CRITICAL**: 必须在一条消息中同时发起两个 Bash 后台调用，run_in_background: true。

**Bash 调用 1（Codex 后端分析）**：
```bash
~/.ghostcode/bin/ghostcode-wrapper --backend codex --workdir "$(pwd)" --group-id "$GHOSTCODE_GROUP_ID" --timeout 600 --stdin <<'CODEX_TASK'
ROLE_FILE: ~/.ghostcode/prompts/codex-analyzer.md

你正在为 GhostCode 项目（Rust 核心 + TS Plugin 多 Agent 协作平台）提供 Rust 核心实现参考。

请针对当前实施任务，提供：
1. Rust 实现参考方案：数据结构设计、核心算法逻辑、关键函数签名
2. 注意事项：内存安全、并发安全、错误处理的具体建议
3. 示例代码片段：仅作参考，实际实现需人工重写为生产级代码

注意：输出仅作原型参考，最终代码必须由人工重写，确保符合项目规范（中文注释、Atlas.oi 署名）。
CODEX_TASK
```

**Bash 调用 2（Gemini 前端分析）**：
```bash
~/.ghostcode/bin/ghostcode-wrapper --backend gemini --workdir "$(pwd)" --group-id "$GHOSTCODE_GROUP_ID" --timeout 600 --stdin <<'GEMINI_TASK'
ROLE_FILE: ~/.ghostcode/prompts/gemini-analyzer.md

你正在为 GhostCode 项目（Rust 核心 + TS Plugin 多 Agent 协作平台）提供 TS Plugin 实现参考。

请针对当前实施任务，提供：
1. TypeScript 实现参考方案：类型定义、函数实现、接口设计
2. 注意事项：类型安全、hook 生命周期、IPC 协议合规的具体建议
3. 示例代码片段：仅作参考，实际实现需人工重写为生产级代码

注意：输出仅作原型参考，最终代码必须由人工重写，确保符合项目规范（中文注释、Atlas.oi 署名）。
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

**MANDATORY 原则**：外部模型输出仅作原型参考，必须人工重写为生产级代码：
- 移除冗余
- 确保命名清晰简洁
- 对齐项目风格（中文注释、Atlas.oi 署名）
- 验证无新依赖引入

#### 副作用审查（应用前必须执行）

在将变更应用到代码库前，逐项验证：

- [ ] 变更范围不超过计划文件中的文件范围
- [ ] 不影响计划文件范围外的模块
- [ ] 未引入新的外部依赖（Cargo.toml / package.json 未变）
- [ ] 未破坏现有接口（IPC 协议向后兼容）

若发现问题，修正后重新验证。

#### 多模型交叉审查（PARALLEL）

##### 开启 Session Gate（MANDATORY，不可跳过）

调用 ghostcode_session_gate_open(command_type="execute_review", required_models=["codex", "gemini"])
→ 得到 session_id，后续所有 submit/close 都使用此 session_id
若 Daemon 离线 → 终止流程，报告错误「GhostCode Daemon 未运行」

**CRITICAL**: 必须在一条消息中同时发起两个 Bash 后台调用，run_in_background: true。

**Bash 调用 1（Codex 代码审查）**：
```bash
~/.ghostcode/bin/ghostcode-wrapper --backend codex --workdir "$(pwd)" --group-id "$GHOSTCODE_GROUP_ID" --timeout 600 --stdin <<'CODEX_REVIEW'
ROLE_FILE: ~/.ghostcode/prompts/codex-reviewer.md

你正在对 GhostCode 项目的代码变更进行审查。

请审查以下维度：
1. 正确性：逻辑错误、边界条件、off-by-one 错误
2. 安全性：Rust unsafe 块使用、注入风险、权限问题
3. 规范合规性：约束集 HC-N 满足情况、TDD 覆盖率

输出 JSON findings：
{
  "findings": [
    {
      "severity": "Critical|Warning|Info",
      "dimension": "logic|security|compliance",
      "file": "文件路径",
      "line": 行号,
      "description": "问题描述",
      "fix_suggestion": "修复建议"
    }
  ]
}
CODEX_REVIEW
```

**Bash 调用 2（Gemini 代码审查）**：
```bash
~/.ghostcode/bin/ghostcode-wrapper --backend gemini --workdir "$(pwd)" --group-id "$GHOSTCODE_GROUP_ID" --timeout 600 --stdin <<'GEMINI_REVIEW'
ROLE_FILE: ~/.ghostcode/prompts/gemini-reviewer.md

你正在对 GhostCode 项目的代码变更进行审查。

请审查以下维度：
1. 可维护性：可读性、代码复杂度、函数长度
2. 模式一致性：与项目现有风格对齐、命名规范、注释质量
3. 集成影响：跨模块影响、IPC 协议兼容性、接口变更影响

输出 JSON findings：
{
  "findings": [
    {
      "severity": "Critical|Warning|Info",
      "dimension": "maintainability|patterns|integration",
      "file": "文件路径",
      "line": 行号,
      "description": "问题描述",
      "fix_suggestion": "修复建议"
    }
  ]
}
GEMINI_REVIEW
```

等待两个后台任务完成。处理所有 Critical 发现后继续。

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

调用 ghostcode_session_gate_close(session_id=<session_id>)
→ 返回合并输出（含 partial 标记和 missing_models 列表）
→ 若 SESSION_INCOMPLETE → 终止，必须补全 missing_models
→ 若 partial=true → 报告顶部标注 WARNING PARTIAL_SESSION（有模型使用了 bypass）

#### 上下文检查点

完成一个 Phase 后报告上下文使用量：
- 低于 80K：询问用户「继续下一个 Phase？」
- 接近 80K：建议「运行 `/clear` 后恢复 `gc:execute <任务名>`」

### Step S2: 归档闭环

当所有任务完成后：

1. 运行验收测试确认全部通过：
   ```bash
   # Rust 验收
   cargo test && cargo build

   # TS 验收
   pnpm test && pnpm typecheck
   ```

2. 写入执行报告到 `.claude/execute/<任务名>-report.md`，包含：
   - 变更摘要（每个任务的完成状态）
   - TDD 执行记录（Red/Green/Refactor 各阶段结果）
   - 审查发现与修复记录

3. 归档变更记录：
   ```bash
   mkdir -p .claude/archive/
   cp -r .claude/plan/<任务名>.md .claude/archive/<任务名>-$(date +%Y%m%d).md
   ```

---

## Step 3（公共）: 汇总报告

无论 parallel 还是 serial 分支，最终都输出：

- 变更摘要：修改的文件列表、新增/删除行数
- 测试状态：cargo test / pnpm test 结果
- 建议：运行 `gc:review` 进行代码审查

## Exit Criteria

### Parallel 模式
- [ ] Daemon 健康检查通过
- [ ] 所有 Builder 任务完成（或明确失败并记录原因）
- [ ] Rust 任务：cargo build 零警告 + cargo test 全通过
- [ ] TS 任务：pnpm typecheck + pnpm test 全通过
- [ ] 变更摘要已输出
- [ ] Team 已清理

### Serial 模式
- [ ] 计划文件中所有任务均完成
- [ ] 每个任务均通过 TDD（Red -> Green -> Refactor）
- [ ] Rust 任务：cargo test 全通过 + cargo build 零警告
- [ ] TS 任务：pnpm test 全通过 + pnpm typecheck 零错误
- [ ] 多模型审查通过（无 Critical 发现）
- [ ] 副作用审查确认无回归
- [ ] 变更已归档
