# GhostCode 实现参考：多模型路由 + codeagent-wrapper

> 源码来源：`/Users/oi/CodeCoding/Code/github/claude-plugin/ccg-workflow/`
> 验证时间：2026-02-28
> 用途：GhostCode 用 Rust 重新实现多模型路由的参考

---

## 一、codeagent-wrapper 完整执行流程

### 调用链

文件：`codeagent-wrapper/main.go:327-493`

```
1. parseArgs() → Config
2. selectBackendFn(backend) → Backend 接口
3. resolveTimeout() → CODEX_TIMEOUT 环境变量（默认 7200 秒）
4. shouldUseStdin() → 检查特殊字符/长度 >800
5. buildCodexArgsFn(cfg, targetArg) → 子进程参数
6. ROLE_FILE 注入 → 正则替换任务文本中的 ROLE_FILE 引用
7. runTaskFn(taskSpec) → 启动子进程
8. 输出 message + SESSION_ID
```

### stdin 触发条件

文件：`codeagent-wrapper/utils.go:50-58` + `main.go:441-464`

task 中含有 `\n \\ " ' \` $` 任意特殊字符，或长度 > 800 字节

### ROLE_FILE 注入

文件：`codeagent-wrapper/utils.go:75-117`

```
正则 (?m)^ROLE_FILE:\s*(.+)$ 匹配后读取文件内容替换整行
在 task 发送给子进程之前完成
```

---

## 二、三个后端的实际 CLI 命令

### Codex 后端

文件：`codeagent-wrapper/backend.go:25-27` + `executor.go:757-799`

```bash
# 新会话
codex e --dangerously-bypass-approvals-and-sandbox --skip-git-repo-check \
    -C <workdir> --json "<task>"

# resume
codex e --dangerously-bypass-approvals-and-sandbox --skip-git-repo-check \
    --json resume <session_id> "<task>"
```

注意：resume 模式下不传 `-C`

### Claude 后端

文件：`codeagent-wrapper/backend.go:84-108`

```bash
# 新会话
claude -p --dangerously-skip-permissions --setting-sources "" \
    --output-format stream-json --verbose "<task>"

# resume
claude -p --dangerously-skip-permissions --setting-sources "" \
    -r <session_id> --output-format stream-json --verbose "<task>"
```

`--setting-sources ""` 防止递归调用 CLAUDE.md/skills

### Gemini 后端

文件：`codeagent-wrapper/backend.go:120-145`

```bash
# 新会话
gemini -m <model> -o stream-json -y -p "<task>"

# resume
gemini -m <model> -o stream-json -y -r <session_id> -p "<task>"
```

### 三后端对比

| 特性 | Codex | Claude | Gemini |
|------|-------|--------|--------|
| 命令 | `codex e` | `claude -p` | `gemini` |
| 工作目录 | `-C <dir>` | `cmd.Dir` | `cmd.Dir` |
| 权限跳过 | `--dangerously-bypass-approvals-and-sandbox` | `--dangerously-skip-permissions` | `-y` |
| 流式输出 | `--json` | `--output-format stream-json --verbose` | `-o stream-json` |
| resume | `resume <id>` (位置参数) | `-r <id>` | `-r <id>` |
| model | 无 | 无 | `-m <model>` |
| 防递归 | 无 | `--setting-sources ""` | 无 |

---

## 三、并行执行引擎

### DAG 拓扑排序

文件：`codeagent-wrapper/executor.go:287-351`

```
1. 建 indegree 图 + 邻接表
2. 入度为 0 的节点入队（BFS）
3. 层遍历：每层可并行，层间顺序依赖
4. 环检测：processed != len(tasks)
输出：[][]TaskSpec（层数组）
```

### goroutine pool + 信号量

文件：`executor.go:353-515`

```go
sem = make(chan struct{}, workerLimit)  // 缓冲 channel 作信号量
// 逐层处理：层间串行，层内并行
for _, layer := range layers {
    var wg sync.WaitGroup
    for _, task := range layer {
        wg.Add(1)
        go func(ts TaskSpec) {
            defer wg.Done()
            acquireSlot()       // 信号量限流
            defer releaseSlot()
            // 执行任务 ...
        }(task)
    }
    wg.Wait()  // 等待本层完成
}
```

默认 `CODEAGENT_MAX_PARALLEL_WORKERS=0`（无限制），最大 100

### 超时处理

文件：`executor.go:960-1173`

```
5 个 channel 同时 select:
- waitCh: 子进程正常退出
- ctx.Done(): timeout 或 Ctrl+C → SIGTERM + 5秒后 SIGKILL
- messageTimerCh: agent_message 后 5s 强制终止
- completeSeen: turn.completed → 启动 5s 倒计时
- messageSeen: agent_message 事件
```

---

## 四、SESSION_ID 复用完整链路

### 提取

文件：`codeagent-wrapper/parser.go:160-163`

```go
if event.SessionID != "" && threadID == "" {
    threadID = event.SessionID  // 第一次出现就锁定
}
```

| 后端 | 事件类型 | 字段名 |
|------|---------|--------|
| Codex | `thread.started` | `thread_id` |
| Claude | `result` | `session_id` |
| Gemini | `init` | `session_id` |

### 输出

文件：`main.go:483-485`

```go
fmt.Printf("\n---\nSESSION_ID: %s\n", result.SessionID)
```

### Resume

```go
// Codex: codex e ... resume <id> <task>
// Claude: claude -p ... -r <id> <task>
// Gemini: gemini ... -r <id> -p <task>
```

---

## 五、JSON Stream 解析

### UnifiedEvent

文件：`codeagent-wrapper/parser.go:72-90`

每行 JSON 解析一次，通过字段存在性判断后端类型：
- Codex：有 `thread_id` 或 `item` 字段
- Claude：有 `subtype` 或 `result` 字段
- Gemini：有 `role` 或 `delta` 或 `status` 字段

### 消息提取差异

- **Codex**：`item.completed` 事件，`item.type=agent_message` → 完整块输出
- **Claude**：`result` 字段 → 完成时一次性输出
- **Gemini**：多个 `content` 字段 → 流式 delta 拼接

---

## 六、代码主权（提示词约定）

### 出现位置（9个命令模板）

文件列表：`workflow.md:189`, `execute.md:14,293`, `plan.md:15,258`, `analyze.md:161`, `frontend.md:165`, `backend.md:165`, `debug.md:155`, `review.md:139`, `test.md:183`

核心规则：`外部模型对文件系统**零写入权限**，所有修改由 Claude 执行`

### Claude 重写 Diff 的 5 步流程

文件：`templates/commands/execute.md:208-237`

```
1. 读取 Diff：解析外部模型的 Unified Diff Patch
2. 思维沙箱：模拟应用，检查一致性
3. 重构清理：脏原型 → 企业发布级代码
4. 最小作用域：变更仅限需求范围
5. 应用变更：使用 Edit/Write 工具落盘
```

### 信任规则

`execute.md:295-296`：后端以 Codex 为准，前端以 Gemini 为准

---

## 七、并行任务格式

文件：`codeagent-wrapper/config.go:113-185`

```
---TASK---
id: task1
workdir: /path/to/dir
backend: codex
dependencies: dep1,dep2
---CONTENT---
实际任务内容
---TASK---
id: task2
session_id: xxx123    ← 有 session_id 自动切为 resume
---CONTENT---
...
```

---

## 八、Rust 重实现对照表

| 组件 | Go 实现 | Rust 建议 |
|------|---------|-----------|
| Backend 接口 | `Backend` interface (策略模式) | `trait Backend { fn name(); fn command(); fn build_args(); }` |
| 参数解析 | 手动 token 扫描 | `clap` crate |
| 子进程管理 | `exec.Command` + `cmd.Start()` | `tokio::process::Command` |
| stdin 管道 | `cmd.StdinPipe()` + goroutine | `child.stdin.take()` + `tokio::io::AsyncWrite` |
| JSON stream | `bufio.Scanner` + `json.Unmarshal` | `tokio::io::BufReader::lines()` + `serde_json` |
| 并行执行 | goroutine + `sync.WaitGroup` + channel 信号量 | `tokio::spawn` + `tokio::sync::Semaphore` |
| DAG 排序 | BFS 层遍历 | 相同算法，`VecDeque` 队列 |
| 超时 | `context.WithTimeout` + select | `tokio::time::timeout` |
| 信号处理 | `signal.Notify(SIGINT, SIGTERM)` | `tokio::signal::ctrl_c()` + `nix::sys::signal` |
