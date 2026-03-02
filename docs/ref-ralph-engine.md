# GhostCode 实现参考：Ralph 验证引擎

> 源码来源：`/Users/oi/CodeCoding/Code/github/claude-plugin/oh-my-claudecode/`
> 验证时间：2026-02-28
> 用途：GhostCode 用 Rust 重新实现 Ralph 验证循环的参考

---

## 一、触发机制

### 关键词触发（主路径）

- 文件：`src/hooks/keyword-detector/index.ts:48`
- 正则：`/\b(ralph)\b(?!-)/i`
- 注册在 `UserPromptSubmit` Hook，每次用户提交 prompt 时检测

### Skill 触发（备选路径）

- 文件：`src/hooks/bridge.ts:1059-1066`
- 调用 `/oh-my-claudecode:ralph` 或 ralplan 规划后 `Skill("oh-my-claudecode:ralph")` 调用

### Ralplan 门控（模糊任务保护）

- 若 prompt 有效词数 <= 15 且无具体文件路径/符号名锚点 → 重定向为 `ralplan`（共识规划）
- 绕过方式：`force: ralph ...` 或 `! ralph ...`

### 激活入口代码

```typescript
// src/hooks/bridge.ts:389-396
const { createRalphLoopHook } = await import("./ralph/index.js");
const hook = createRalphLoopHook(directory);
hook.startLoop(sessionId, promptText);
```

---

## 二、7 项标准检查

### 定义位置

文件：`src/features/verification/index.ts:27-87`

### 检查项详情

| 检查项 | ID | 判定方式 | 自动化 |
|--------|-----|---------|--------|
| BUILD | `build` | 运行构建命令，检查退出码 | 有命令自动执行 |
| TEST | `test` | 运行测试命令，0 失败 | 有命令自动执行 |
| LINT | `lint` | 运行 lint 命令，0 错误 | 有命令自动执行 |
| FUNCTIONALITY | `functionality` | 特性按描述工作 | 无命令，Agent 提供证据 |
| ARCHITECT | `architect` | Architect agent 审批 | 无命令，Agent 调用子 agent |
| TODO | `todo` | 0 个 pending/in_progress 任务 | 无命令，Agent 提供证据 |
| ERROR_FREE | `error_free` | 0 个未处理错误 | 无命令，Agent 提供证据 |

### 有命令 vs 无命令的区分

```typescript
// src/features/verification/index.ts:129-162
// 有命令的检查：执行命令 → 捕获 stdout/stderr → 通过退出码判断
// 无命令的检查：返回 { passed: false, metadata: { requiresManualVerification: true } }
```

### 验证运行模式

- 默认并行运行（`parallel: true`）
- 可配置为顺序或 failFast 模式

---

## 三、5 分钟证据时效

### 实现代码

文件：`src/features/verification/index.ts:278-283`

```typescript
const fiveMinutesAgo = new Date(Date.now() - 5 * 60 * 1000);
if (evidence.timestamp < fiveMinutesAgo) {
  issues.push('Evidence is stale (older than 5 minutes)');
  recommendations.push('Re-run verification to get fresh evidence');
}
```

### 过期处理

- 过期证据导致 `valid: false`
- 不自动删除，但该检查项视为未通过
- 建议重新运行验证

---

## 四、验证循环逻辑

### Architect 审批循环

文件：`src/hooks/ralph/verifier.ts:38`

- **最多 3 次验证尝试**（`DEFAULT_MAX_VERIFICATION_ATTEMPTS = 3`）

### 循环流程

```
1. 运行 7 项检查
2. 检查是否全部通过
3. 若未通过 → Agent 修复 → 回到 1
4. 若全部通过 → 调用 Architect 子 agent 审批
5. Architect 批准 → 清除状态，退出循环
6. Architect 拒绝 → verification_attempts += 1
7. attempts >= 3 → 强制接受（force-accepting）
8. attempts < 3 → 注入 continuation prompt → 回到 1
```

### Architect 审批检测

文件：`src/hooks/ralph/verifier.ts:246-276`

- 在 session transcript 末尾 **32KB** 内扫描
- 批准标记：`/<architect-approved>.*?VERIFIED_COMPLETE.*?<\/architect-approved>/is`
- 拒绝标记：6 种模式（`architect.*rejected`、`issues? found`、`not yet complete` 等）

### 失败后重试

文件：`src/hooks/persistent-mode/index.ts:501-518`

Architect 拒绝后，Stop Hook 注入 `getArchitectRejectionContinuationPrompt` 提示词，强制 Agent 继续修复。

---

## 五、Stop Hook 交互（关键机制）

### 优先级

```
Ralph (Priority 1) > Autopilot (1.5) > Ultrawork (2) > SkillState (3)
```

### 软阻止策略

文件：`src/hooks/persistent-mode/index.ts:848-857`

```typescript
// 永远不返回 continue: false（软阻止）
return {
  continue: true,              // 永远允许停止
  message: result.message      // 但注入强制继续的提示词
};
```

### 绕过条件（按优先级）

1. Context limit stop（上下文限制） → 放行（防止 compaction 死锁）
2. 显式 cancel 命令 → 放行
3. Cancel signal 文件存在且未过期（30秒 TTL） → 放行
4. 用户中止 → 放行
5. Rate limit（429/quota） → 放行并提示暂停

### 迭代超限处理（关键！）

文件：`src/hooks/persistent-mode/index.ts:536-541`

```typescript
// 迭代超限不会停止 Ralph，而是自动扩展上限！
if (state.iteration >= state.max_iterations) {
  state.max_iterations += 10;  // 延长 10 次
}
```

**唯一真正停止 Ralph 的方式**：`/oh-my-claudecode:cancel`（写入 cancel-signal 文件，30秒 TTL）

---

## 六、状态管理

### 状态文件路径

```
.omc/sessions/<sessionId>/ralph.json              -- Ralph 循环状态
.omc/sessions/<sessionId>/ralph-verification.json  -- 验证状态
.omc/sessions/<sessionId>/cancel-signal.json       -- 取消信号（30秒TTL）
```

旧版路径：`.omc/state/ralph-state.json`

### Ralph 状态结构

```typescript
// src/hooks/ralph/loop.ts:76-97
interface RalphLoopState {
  active: boolean;             // 是否激活
  iteration: number;           // 当前迭代次数
  max_iterations: number;      // 最大迭代（默认10，超限自动+10）
  started_at: string;          // 开始时间 ISO
  prompt: string;              // 原始 prompt
  session_id?: string;         // 绑定的 session ID
  project_path?: string;       // 项目路径
  prd_mode?: boolean;          // PRD 模式
  current_story_id?: string;   // 当前 PRD story ID
  linked_ultrawork?: boolean;  // 联动 ultrawork
}
```

### 会话隔离

```typescript
// src/hooks/ralph/loop.ts:157-163
if (state.session_id && state.session_id !== sessionId) {
  return null;  // 严格按 session_id 匹配，不同会话互不可见
}
```

---

## 七、GhostCode Rust 实现要点

| 模块 | omc 实现 | Rust 实现建议 |
|------|---------|--------------|
| 触发 | UserPromptSubmit hook + 关键字正则 | TS 薄壳 Hook 匹配后通过 IPC 通知 Rust daemon |
| 循环控制 | Stop hook 注入 continuation prompt | Rust daemon 管理状态，TS 层读取并注入 |
| 验证 | 7项检查（3项自动+4项手动） | Rust 执行 build/test/lint 命令，手动项通过 MCP 收集 |
| 时效 | evidence.timestamp 比较 | `chrono::Utc::now() - Duration::minutes(5)` |
| Architect | 扫描 transcript 32KB 正则匹配 | Rust 正则引擎 + transcript 文件 tail 读取 |
| 迭代上限 | 默认 10，超限 +10，永不停止 | 状态文件 `max_iterations` 字段 |
| 取消 | cancel-signal 文件 30秒 TTL | 状态文件 + TTL 检查 |
| 状态存储 | JSON 文件（per-session 目录） | serde_json + 文件系统 |
