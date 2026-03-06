---
name: spec-impl
description: 按规范执行 + TDD 驱动 + 多模型协作 + 归档：完整实施闭环
aliases:
  - spec impl
  - 规范实施
do_not_use_when: spec-plan 任务清单不存在、或处于 team-exec 并行执行中
---

## Purpose

按照 spec-plan 产出的任务清单（tasks.md）进行 TDD 驱动的实施，完成后多模型交叉
审查，最终归档变更记录，形成完整的需求 → 研究 → 规划 → 实施 → 归档闭环。

实施是纯机械执行——所有决策已在 spec-plan 阶段完成。每个 Phase 完成后检查上下文
用量，避免超出限制导致实施中断。

GhostCode 特殊性：
- Rust 实施必须遵循 TDD：先写 `_test.rs` 文件（Red），再写实现（Green），最后重构
- TS Plugin 实施同样遵循 TDD：先写 `.test.ts`，再写实现
- Rust 外部模型产出只作为参考，必须人工重写为生产级代码

## Use When

- 用户输入 "spec impl"、"ccg:spec-impl"、"规范实施"
- 已完成 spec-plan，tasks.md 存在
- 需要按计划实施并归档

## Do Not Use When

- spec-plan 尚未完成（tasks.md 不存在）
- 在 team-exec 并行实施环境中（Builder 直接按 team-plan 执行，不用 spec-impl）
- 任务已全部标记为 [x]（变更已完成）

## Steps

### Step 0: MANDATORY Prompt 增强

确认变更名称。读取 `.claude/spec-changes/<变更名>/tasks.md`。
若不存在，提示先运行 `/spec-plan <变更名>`，终止。

### Step 1: 选择 Phase

运行查看当前任务状态：

```bash
# 查看未完成的任务
grep -n "\- \[ \]" .claude/spec-changes/<变更名>/tasks.md
```

识别**最小可验证 Phase**：不一次完成所有任务，控制上下文窗口。
宣告：「开始实施 Phase X：[任务组名称]」

### Step 2: TDD 实施流程（每个任务）

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
// import 目标函数（可能还不存在，先写导入）

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

### Step 3: 外部模型协作（可选）

对复杂算法或性能敏感代码，可调用外部模型获取参考实现。

**CRITICAL**: 必须在一条消息中同时发起两个 Bash 后台调用，run_in_background: true。

**Bash 调用 1（Codex 后端分析）**：
```bash
~/.ghostcode/bin/ghostcode-wrapper --backend codex --workdir "$(pwd)" --timeout 600 --stdin <<'CODEX_TASK'
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
~/.ghostcode/bin/ghostcode-wrapper --backend gemini --workdir "$(pwd)" --timeout 600 --stdin <<'GEMINI_TASK'
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

**失败处理**：若 wrapper 退出码非 0（如 CLI 不可用退出码 127），log 错误并继续执行（用 Claude 自身实现替代），不终止整个流程。

**MANDATORY 原则**：外部模型输出仅作原型参考，必须人工重写为生产级代码：
- 移除冗余
- 确保命名清晰简洁
- 对齐项目风格（中文注释、Atlas.oi 署名）
- 验证无新依赖引入

### Step 4: 副作用审查（应用前必须执行）

在将变更应用到代码库前，逐项验证：

- [ ] 变更范围不超过 tasks.md 中的文件范围
- [ ] 不影响 tasks.md 范围外的模块
- [ ] 未引入新的外部依赖（Cargo.toml / package.json 未变）
- [ ] 未破坏现有接口（IPC 协议向后兼容）

若发现问题，修正后重新验证。

### Step 5: 多模型交叉审查（PARALLEL）

**CRITICAL**: 必须在一条消息中同时发起两个 Bash 后台调用，run_in_background: true。

**Bash 调用 1（Codex 代码审查）**：
```bash
~/.ghostcode/bin/ghostcode-wrapper --backend codex --workdir "$(pwd)" --timeout 600 --stdin <<'CODEX_REVIEW'
ROLE_FILE: ~/.ghostcode/prompts/codex-reviewer.md

你正在对 GhostCode 项目（Rust 核心 + TS Plugin 多 Agent 协作平台）的代码变更进行审查。

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
~/.ghostcode/bin/ghostcode-wrapper --backend gemini --workdir "$(pwd)" --timeout 600 --stdin <<'GEMINI_REVIEW'
ROLE_FILE: ~/.ghostcode/prompts/gemini-reviewer.md

你正在对 GhostCode 项目（Rust 核心 + TS Plugin 多 Agent 协作平台）的代码变更进行审查。

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

等待两个后台任务完成：使用 TaskOutput(block: true, timeout: 600000) 读取各自结果。

**失败处理**：若 wrapper 退出码非 0（如 CLI 不可用退出码 127），log 错误并继续执行（用 Claude 自身审查替代），不终止整个流程。

处理所有 Critical 发现后继续。

### Step 6: 更新任务状态

在 tasks.md 中将已完成任务标记为 `[x]`：

```markdown
- [x] Task 1: <名称>
```

### Step 7: 上下文检查点

完成一个 Phase 后报告上下文使用量：
- 低于 80K：询问用户「继续下一个 Phase？」
- 接近 80K：建议「运行 `/clear` 后恢复 `/spec-impl <变更名>`」

### Step 8: 归档（所有任务完成后）

当 tasks.md 中所有任务均标记为 `[x]` 时，归档变更：

```bash
# 将变更记录归档
mkdir -p .claude/archive/
cp -r .claude/spec-changes/<变更名>/ .claude/archive/<变更名>-$(date +%Y%m%d)/
```

输出归档完成报告，包含：变更摘要、成功判据对照、归档路径。

## Exit Criteria

- [ ] tasks.md 中所有任务均标记为 `[x]`
- [ ] 每个任务均通过 TDD（Red → Green → Refactor）
- [ ] Rust 任务：cargo test 全通过 + cargo build 零警告
- [ ] TS 任务：pnpm test 全通过 + pnpm typecheck 零错误
- [ ] 多模型审查通过（无 Critical 发现）
- [ ] 副作用审查确认无回归
- [ ] 变更已归档到 `.claude/archive/`
