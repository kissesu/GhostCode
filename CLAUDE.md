# GhostCode - 多 Agent 协作开发平台

## 项目概述

GhostCode 是一个基于 Rust 核心 + TypeScript 薄壳的多 Agent 协作开发平台，作为 Claude Code Plugin 分发。
融合三个开源项目的核心优势：
- **CCCC** 的通信内核（Daemon + 消息可靠投递 + 多 Runtime 支持）
- **ccg-workflow** 的代码安全策略（Claude 独占写入 + 多模型智能路由 + team-*/spec-* 流程）
- **oh-my-claudecode** 的用户体验（Magic Keywords + Ralph 验证循环 + HUD 状态栏）

## 技术栈

- **核心引擎**: Rust (tokio 异步运行时)
- **Plugin 层**: TypeScript (Claude Code Plugin)
- **通信协议**: JSON-RPC over Unix socket / stdio
- **配置格式**: TOML
- **构建工具**: cargo (Rust) + pnpm (TS)

## 项目结构

```
GhostCode/
  src/
    core/          -- Rust 核心引擎
    plugin/        -- TypeScript Claude Code Plugin 薄壳
  docs/            -- 项目文档
    research.md    -- 前期研究约束集
  .github/
    workflows/     -- CI/CD
```

## 开发规范

- 遵循全局 CLAUDE.md 规范
- Rust 代码遵循 clippy 标准
- 所有注释使用中文
- 作者署名: Atlas.oi

## TDD 严格执行规范（强制）

所有任务（T14-T20 及后续）必须严格遵循 TDD + PBT 混合驱动流程，禁止先写实现后补测试。

### 三阶段工作流

```
Red    → 先写测试文件（xxx_test.rs），所有测试用例编译通过但断言失败
Green  → 写最少实现代码，让所有测试通过
Refactor → 集成到已有模块 + 重构，测试仍通过
```

### team-plan 子任务排序规则

```
Layer 1: 测试文件（tests/xxx_test.rs）—— 包含全部测试用例，依赖项用 stub/mock
Layer 2: 核心实现代码（src/xxx.rs）—— 让测试从 Red 变 Green
Layer 3: 集成代码（修改已有文件：mod.rs, server.rs, dispatch.rs 等）
Layer 4: 最终验证 —— cargo build 零警告 + cargo test 全通过
```

### Builder spawn prompt 必须包含

1. 明确要求先创建测试文件
2. 要求运行 `cargo test` 确认测试**失败**（Red 阶段验证）
3. 再写实现代码让测试通过（Green 阶段验证）
4. 最后集成和重构

### 历史教训

- T13 投递引擎：team-plan 按实现依赖排列（代码先于测试），违反了 TDD 流程，已纠正

## 参考项目源码（本地 clone）

三个参考项目已 clone 到本地，所有功能参考、逻辑借鉴、架构分析必须基于这些源码：

```
/Users/oi/CodeCoding/Code/github/claude-plugin/
  cccc/                -- CCCC 源码 (Apache-2.0)
  ccg-workflow/        -- ccg-workflow 源码 (MIT)
  oh-my-claudecode/    -- oh-my-claudecode 源码 (MIT)
```

| 项目 | 本地路径 | GitHub | 许可证 |
|------|---------|--------|--------|
| CCCC | `/Users/oi/CodeCoding/Code/github/claude-plugin/cccc` | [ChesterRa/cccc](https://github.com/ChesterRa/cccc) | Apache-2.0 |
| ccg-workflow | `/Users/oi/CodeCoding/Code/github/claude-plugin/ccg-workflow` | [fengshao1227/ccg-workflow](https://github.com/fengshao1227/ccg-workflow) | MIT |
| oh-my-claudecode | `/Users/oi/CodeCoding/Code/github/claude-plugin/oh-my-claudecode` | [Yeachan-Heo/oh-my-claudecode](https://github.com/Yeachan-Heo/oh-my-claudecode) | MIT |

## 代码溯源规范（强制）

### 禁止事项
1. **禁止臆想功能逻辑** - 不可凭记忆或猜测描述三个项目的实现细节
2. **禁止胡说造假** - 所有关于参考项目的技术描述必须有源码依据
3. **禁止凭空编造接口** - 不可假设参考项目的 API/接口/协议，必须从源码验证

### 强制要求
1. **源码优先** - 参考三个项目的功能逻辑时，必须先从本地 clone 的源码中查询验证
2. **标注出处** - 借鉴的每个功能模块必须标注来源项目和具体源码文件路径
3. **Glob/Grep/Read 验证** - 使用工具在参考项目源码中搜索和阅读，确认实际实现后再引用
4. **差异标注** - 如果 GhostCode 的实现与参考项目不同，必须说明差异原因

### 溯源格式
在文档和代码注释中，引用参考项目时使用以下格式：
```
// 参考: cccc/src/daemon/ledger.py:45-80 - append-only 事件账本实现
// 参考: ccg-workflow/src/wrapper/main.go:120 - 会话复用机制
// 参考: oh-my-claudecode/src/skills/ralph.ts:30-95 - Ralph 验证循环
```
