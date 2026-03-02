# Team Plan: T16 Plugin 项目初始化

## 概述

为 GhostCode 初始化 TypeScript Plugin 脚手架，建立完整目录结构、配置文件和占位源码文件。
Plugin 作为 Claude Code Plugin 分发，是连接 Rust 核心 Daemon 和 Claude Code 宿主环境的薄壳。
本任务只创建骨架，不实现业务逻辑（Daemon 管理在 T17，IPC 通信在 T18）。

**验收标准**: 在 `src/plugin/` 目录下执行 `pnpm install && pnpm build` 零错误。

## Codex 分析摘要

Codex CLI 不可用，由 Claude 自行分析。

## Gemini 分析摘要

批量计划生成模式，跳过多模型分析。

## 技术方案

### 参考溯源

- 参考: `oh-my-claudecode/.claude-plugin/plugin.json` - Claude Code Plugin 配置格式（skills + mcpServers 字段）
- 参考: `ccg-workflow/package.json` - ESM 模块配置（`"type": "module"`、`unbuild` 构建工具）
- 参考: `ccg-workflow/build.config.ts` - unbuild 配置样式（entries + declaration + rollup）
- 参考: `oh-my-claudecode/package.json` - exports 字段 + engines.node 约束模式

### 架构决策

1. **构建工具选 `tsup` 而非 `unbuild`**: T16 规格明确指定使用 tsup；tsup 配置更简单，适合初期脚手架
2. **ESM only**: `"type": "module"`，不输出 CJS（Claude Code Plugin 运行时支持 ESM）
3. **Plugin 配置放 `src/plugin/.claude/settings.json`** 而非 `.claude-plugin/plugin.json`：GhostCode 的设计是单仓库多产物，Plugin 配置随 plugin 目录管理
4. **占位文件导出空函数**: daemon.ts 和 ipc.ts 各导出一个空函数占位，让 index.ts 可以安全导入，`pnpm build` 不报错
5. **Node 版本约束 >=20**: 与 oh-my-claudecode 一致，保证 ESM + top-level await 支持

### 目录结构（完整）

```
src/plugin/
  package.json           -- 包配置（name: ghostcode，ESM，tsup）
  tsconfig.json          -- TypeScript 严格模式配置
  tsup.config.ts         -- 构建入口配置
  src/
    index.ts             -- Plugin 入口，导出所有公开 API
    daemon.ts            -- Daemon 管理占位（T17 实现）
    ipc.ts               -- IPC 通信占位（T18 实现）
    hooks/
      index.ts           -- Hook 注册占位
  .claude/
    settings.json        -- Claude Code Plugin 配置声明
```

## 子任务列表

---

### Task 1: 创建 package.json

- **类型**: 配置
- **文件范围**: `src/plugin/package.json`（新建）
- **依赖**: 无
- **实施步骤**:
  1. 创建文件，写入以下完整内容：

```json
{
  "name": "ghostcode",
  "version": "0.1.0",
  "description": "GhostCode - 多 Agent 协作开发平台，Claude Code Plugin",
  "type": "module",
  "main": "dist/index.js",
  "types": "dist/index.d.ts",
  "exports": {
    ".": {
      "import": "./dist/index.js",
      "types": "./dist/index.d.ts"
    }
  },
  "files": [
    "dist",
    ".claude"
  ],
  "scripts": {
    "build": "tsup",
    "dev": "tsup --watch",
    "typecheck": "tsc --noEmit"
  },
  "devDependencies": {
    "@types/node": "^22.0.0",
    "tsup": "^8.0.0",
    "typescript": "^5.7.0"
  },
  "engines": {
    "node": ">=20.0.0"
  },
  "packageManager": "pnpm@10.0.0",
  "author": "Atlas.oi",
  "license": "MIT",
  "keywords": [
    "claude-code",
    "plugin",
    "multi-agent",
    "ghostcode"
  ]
}
```

- **验收标准**: 文件格式合法的 JSON，`pnpm install` 可以读取该文件并安装依赖

---

### Task 2: 创建 tsconfig.json

- **类型**: 配置
- **文件范围**: `src/plugin/tsconfig.json`（新建）
- **依赖**: 无（与 Task 1 并行）
- **实施步骤**:
  1. 创建文件，写入以下完整内容：

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "lib": ["ES2022"],
    "outDir": "dist",
    "rootDir": "src",
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true,
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "noImplicitOverride": true,
    "exactOptionalPropertyTypes": true,
    "skipLibCheck": true,
    "esModuleInterop": true,
    "allowSyntheticDefaultImports": true
  },
  "include": ["src/**/*.ts"],
  "exclude": ["node_modules", "dist"]
}
```

**关键配置说明**:
- `"moduleResolution": "bundler"`: 适配 tsup 的模块解析方式
- `"strict": true`: 开启严格模式（noImplicitAny、strictNullChecks 等）
- `"noUncheckedIndexedAccess": true`: 数组/对象访问默认包含 undefined，更安全
- `"exactOptionalPropertyTypes": true`: 可选属性类型精确匹配

- **验收标准**: `pnpm typecheck` 零错误

---

### Task 3: 创建 tsup.config.ts

- **类型**: 配置
- **文件范围**: `src/plugin/tsup.config.ts`（新建）
- **依赖**: Task 1, Task 2（需要 tsup 已安装，tsconfig 已存在）
- **实施步骤**:
  1. 创建文件，写入以下完整内容：

```typescript
/**
 * @file tsup 构建配置
 * @description GhostCode Plugin 的构建工具配置
 *              tsup 基于 esbuild，支持 ESM 输出和 TypeScript 声明文件生成
 * @author Atlas.oi
 * @date 2026-03-01
 */
import { defineConfig } from "tsup";

export default defineConfig({
  // 构建入口：Plugin 主入口
  entry: ["src/index.ts"],

  // 输出格式：仅 ESM，不输出 CJS
  format: ["esm"],

  // 生成 TypeScript 声明文件（.d.ts）
  dts: true,

  // 生成 Source Map，便于调试
  sourcemap: true,

  // 构建前清空 dist 目录
  clean: true,

  // 目标平台：Node.js 20+
  target: "node20",

  // 不将依赖打包进输出文件（Plugin 运行时由宿主环境提供 node_modules）
  bundle: false,
});
```

- **验收标准**: `pnpm build` 成功输出 `dist/index.js` 和 `dist/index.d.ts`

---

### Task 4: 创建 src/daemon.ts 占位文件

- **类型**: 源码占位
- **文件范围**: `src/plugin/src/daemon.ts`（新建）
- **依赖**: 无（与其他 Task 并行）
- **实施步骤**:
  1. 创建文件，写入以下完整内容：

```typescript
/**
 * @file Daemon 管理模块
 * @description GhostCode Daemon 进程管理的类型定义和接口占位。
 *              具体实现在 T17 完成：包括启动/停止 Daemon、健康检查、Unix socket 连接建立。
 *              当前只导出类型定义和空函数签名，确保 index.ts 可以安全导入。
 * @author Atlas.oi
 * @date 2026-03-01
 */

// Daemon 连接状态枚举
// T17 实现时会扩展此枚举的使用
export type DaemonStatus = "stopped" | "starting" | "running" | "error";

// Daemon 配置选项接口
// T17 实现时会使用此接口初始化 Daemon 连接
export interface DaemonOptions {
  /** Unix socket 路径，用于 IPC 通信 */
  socketPath: string;
  /** 启动超时时间（毫秒） */
  startTimeoutMs?: number;
}

/**
 * 获取当前 Daemon 连接状态（占位）
 *
 * T17 实现时将替换此函数体，实际检查 Daemon 进程状态。
 *
 * @returns 当前 Daemon 状态
 */
export function getDaemonStatus(): DaemonStatus {
  // 占位实现：T17 实现真实逻辑
  return "stopped";
}

/**
 * 启动 GhostCode Daemon 进程（占位）
 *
 * T17 实现时将：
 * 1. 检查是否已有 Daemon 在运行（PID 文件锁）
 * 2. 启动 Rust 核心 Daemon 可执行文件
 * 3. 等待 Unix socket 就绪
 *
 * @param _options Daemon 启动配置选项
 */
export async function startDaemon(_options?: DaemonOptions): Promise<void> {
  // 占位实现：T17 实现真实逻辑
}

/**
 * 停止 GhostCode Daemon 进程（占位）
 *
 * T17 实现时将发送 SIGTERM 信号并等待进程退出。
 */
export async function stopDaemon(): Promise<void> {
  // 占位实现：T17 实现真实逻辑
}
```

- **验收标准**: TypeScript 编译无错误，文件可被 index.ts 导入

---

### Task 5: 创建 src/ipc.ts 占位文件

- **类型**: 源码占位
- **文件范围**: `src/plugin/src/ipc.ts`（新建）
- **依赖**: 无（与其他 Task 并行）
- **实施步骤**:
  1. 创建文件，写入以下完整内容：

```typescript
/**
 * @file IPC 通信模块
 * @description GhostCode Plugin 与 Daemon 之间通过 Unix socket 进行 IPC 通信的类型定义和接口占位。
 *              具体实现在 T18 完成：包括连接建立、JSON-RPC 请求发送、响应解析。
 *              当前只导出类型定义和空函数签名，确保 index.ts 可以安全导入。
 * @author Atlas.oi
 * @date 2026-03-01
 */

// IPC 请求结构（与 Rust Daemon 的 DispatchRequest 对应）
// 参考: crates/ghostcode-daemon/src/dispatch.rs 中的 DispatchRequest 结构
export interface IpcRequest {
  /** 操作名称，如 "ping"、"actor_start" */
  op: string;
  /** 操作参数，任意 JSON 对象 */
  args: Record<string, unknown>;
}

// IPC 响应结构（与 Rust Daemon 的 DispatchResponse 对应）
export interface IpcResponse {
  /** 操作是否成功 */
  ok: boolean;
  /** 响应数据（ok=true 时） */
  data?: unknown;
  /** 错误信息（ok=false 时） */
  error?: string;
}

// IPC 客户端连接句柄接口
// T18 实现时会创建实现此接口的 IpcClient 类
export interface IpcClient {
  /** 发送请求并等待响应 */
  send(request: IpcRequest): Promise<IpcResponse>;
  /** 关闭连接 */
  close(): Promise<void>;
}

/**
 * 连接到 GhostCode Daemon 的 Unix socket（占位）
 *
 * T18 实现时将：
 * 1. 通过 net.createConnection 连接到 Unix socket
 * 2. 建立 JSON-RPC 消息帧（按行分割）
 * 3. 返回可复用的 IpcClient 实例
 *
 * @param _socketPath Unix socket 文件路径
 * @returns IPC 客户端连接句柄
 */
export async function connectIpc(_socketPath: string): Promise<IpcClient> {
  // 占位实现：T18 实现真实逻辑
  throw new Error("IPC 连接未实现，请等待 T18 任务完成");
}

/**
 * 向 Daemon 发送 ping 请求（占位）
 *
 * T18 实现时用于健康检查，验证 Daemon 是否响应。
 *
 * @param _client IPC 客户端连接
 * @returns Daemon 是否存活
 */
export async function ping(_client: IpcClient): Promise<boolean> {
  // 占位实现：T18 实现真实逻辑
  return false;
}
```

- **验收标准**: TypeScript 编译无错误，文件可被 index.ts 导入

---

### Task 6: 创建 src/hooks/index.ts

- **类型**: 源码占位
- **文件范围**: `src/plugin/src/hooks/index.ts`（新建）
- **依赖**: 无（与其他 Task 并行）
- **实施步骤**:
  1. 创建 `src/plugin/src/hooks/` 目录
  2. 创建 `index.ts` 文件，写入以下完整内容：

```typescript
/**
 * @file Claude Code Hook 注册模块
 * @description GhostCode Plugin 的 Hook 注册入口，用于在 Claude Code 生命周期
 *              各阶段注入自定义逻辑。Hook 类型包括：
 *              - PreToolUse: 工具调用前拦截
 *              - PostToolUse: 工具调用后处理
 *              - Notification: 通知事件处理
 *              - Stop: 会话终止处理
 *              当前为占位实现，具体 Hook 逻辑在后续任务中实现。
 * @author Atlas.oi
 * @date 2026-03-01
 */

// Hook 事件类型枚举（与 Claude Code Plugin 协议对应）
export type HookEventType =
  | "PreToolUse"
  | "PostToolUse"
  | "Notification"
  | "Stop";

// Hook 处理函数类型
export type HookHandler = (event: unknown) => Promise<unknown> | unknown;

// 已注册的 Hook 映射表
// Key: HookEventType，Value: 处理函数列表
const registeredHooks = new Map<HookEventType, HookHandler[]>();

/**
 * 注册一个 Hook 处理函数（占位）
 *
 * T17/T18 实现时会在各自模块初始化时调用此函数注册 Hook。
 *
 * @param eventType Hook 事件类型
 * @param handler 处理函数
 */
export function registerHook(
  eventType: HookEventType,
  handler: HookHandler,
): void {
  const existing = registeredHooks.get(eventType) ?? [];
  registeredHooks.set(eventType, [...existing, handler]);
}

/**
 * 获取指定类型的所有已注册 Hook 处理函数
 *
 * @param eventType Hook 事件类型
 * @returns 处理函数列表
 */
export function getHooks(eventType: HookEventType): HookHandler[] {
  return registeredHooks.get(eventType) ?? [];
}

/**
 * 清除所有已注册的 Hook（主要用于测试）
 */
export function clearHooks(): void {
  registeredHooks.clear();
}
```

- **验收标准**: TypeScript 编译无错误，目录结构正确

---

### Task 7: 创建 src/index.ts Plugin 入口

- **类型**: 源码占位
- **文件范围**: `src/plugin/src/index.ts`（新建）
- **依赖**: Task 4, Task 5, Task 6（需要三个占位模块存在）
- **实施步骤**:
  1. 创建文件，写入以下完整内容：

```typescript
/**
 * @file GhostCode Plugin 主入口
 * @description GhostCode Claude Code Plugin 的公开 API 导出入口。
 *              作为 TypeScript 薄壳，本文件聚合三个核心模块的导出：
 *              - daemon: Daemon 进程管理（T17 实现）
 *              - ipc: IPC 通信层（T18 实现）
 *              - hooks: Claude Code Hook 注册（后续任务实现）
 *
 *              Plugin 架构设计：
 *              Claude Code 宿主 → Plugin (index.ts) → IPC → Rust Daemon
 * @author Atlas.oi
 * @date 2026-03-01
 */

// ============================================
// Daemon 管理模块导出
// 占位导出，T17 实现具体逻辑
// ============================================
export type { DaemonStatus, DaemonOptions } from "./daemon.js";
export { getDaemonStatus, startDaemon, stopDaemon } from "./daemon.js";

// ============================================
// IPC 通信模块导出
// 占位导出，T18 实现具体逻辑
// ============================================
export type { IpcRequest, IpcResponse, IpcClient } from "./ipc.js";
export { connectIpc, ping } from "./ipc.js";

// ============================================
// Hook 注册模块导出
// ============================================
export type { HookEventType, HookHandler } from "./hooks/index.js";
export { registerHook, getHooks, clearHooks } from "./hooks/index.js";

// ============================================
// Plugin 版本信息
// ============================================

/** GhostCode Plugin 版本号 */
export const VERSION = "0.1.0";

/** GhostCode Plugin 名称 */
export const PLUGIN_NAME = "ghostcode";
```

**注意**: ESM 模式下导入本地文件必须使用 `.js` 扩展名（TypeScript 编译后目标为 `.js`，tsup 在 ESM 模式下要求显式扩展名）。

- **验收标准**: `pnpm build` 成功，`dist/index.js` 和 `dist/index.d.ts` 正确生成

---

### Task 8: 创建 .claude/settings.json Plugin 配置

- **类型**: 配置
- **文件范围**: `src/plugin/.claude/settings.json`（新建）
- **依赖**: 无（与其他 Task 并行）
- **实施步骤**:
  1. 创建 `src/plugin/.claude/` 目录
  2. 创建 `settings.json` 文件，写入以下完整内容：

```json
{
  "name": "ghostcode",
  "version": "0.1.0",
  "description": "GhostCode 多 Agent 协作开发平台 - Claude Code Plugin",
  "author": "Atlas.oi",
  "license": "MIT",
  "repository": "https://github.com/atlas-oi/GhostCode",
  "keywords": [
    "claude-code",
    "plugin",
    "multi-agent",
    "ghostcode",
    "daemon"
  ],
  "skills": "./skills/",
  "permissions": {
    "filesystem": {
      "read": true,
      "write": false
    },
    "network": false,
    "shell": false
  }
}
```

**字段说明**（参考 oh-my-claudecode/.claude-plugin/plugin.json）:
- `skills`: 指向 skills 目录（T17 后续创建 skill 文件时用）
- `permissions`: 声明 Plugin 所需权限，遵循最小权限原则

- **验收标准**: 文件格式合法的 JSON

---

### Task 9: 执行 pnpm install 并验证 pnpm build

- **类型**: 验证
- **文件范围**: `src/plugin/` 整体
- **依赖**: Task 1 ~ Task 8 全部完成
- **实施步骤**:
  1. 切换到 `src/plugin/` 目录
  2. 执行 `pnpm install` 安装 devDependencies（tsup、typescript、@types/node）
  3. 执行 `pnpm build` 触发 tsup 构建
  4. 验证 `dist/` 目录结构：
     - `dist/index.js` 存在
     - `dist/index.d.ts` 存在
  5. 执行 `pnpm typecheck`（`tsc --noEmit`）确认类型检查零错误
  6. 检查 `dist/index.js` 内容，确认导出正确

**预期 dist/ 结构**:
```
dist/
  index.js        -- ESM 模块，包含所有导出
  index.d.ts      -- 类型声明文件
  index.js.map    -- Source Map（可选）
```

- **验收标准**: `pnpm build` 零错误，`pnpm typecheck` 零错误

---

## 文件冲突检查

| Task | 文件 | 操作 |
|------|------|------|
| Task 1 | `src/plugin/package.json` | 新建 |
| Task 2 | `src/plugin/tsconfig.json` | 新建 |
| Task 3 | `src/plugin/tsup.config.ts` | 新建 |
| Task 4 | `src/plugin/src/daemon.ts` | 新建 |
| Task 5 | `src/plugin/src/ipc.ts` | 新建 |
| Task 6 | `src/plugin/src/hooks/index.ts` | 新建 |
| Task 7 | `src/plugin/src/index.ts` | 新建 |
| Task 8 | `src/plugin/.claude/settings.json` | 新建 |
| Task 9 | — | 验证执行（无文件修改） |

- 所有文件均为新建，无冲突风险
- `src/plugin/` 目录当前为空，可直接写入

## 并行分组

- **Layer 1** (完全并行): Task 1, Task 2, Task 4, Task 5, Task 6, Task 8
  - 这些文件互不依赖，可由多个 Builder 同时创建
- **Layer 2** (依赖 Layer 1 的 Task 4/5/6): Task 7 (index.ts)
  - index.ts 需要导入 daemon.ts、ipc.ts、hooks/index.ts，必须这三个文件先存在
- **Layer 3** (依赖 Layer 1 的 Task 1/2): Task 3 (tsup.config.ts)
  - tsup.config.ts 语义上依赖 tsconfig.json 存在，package.json 中已声明 tsup 依赖
- **Layer 4** (依赖 Layer 1~3 全部): Task 9 (验证)
  - 所有文件创建完毕后才能执行构建验证

**推荐执行顺序**:
```
Layer 1: Task 1 + Task 2 + Task 4 + Task 5 + Task 6 + Task 8 (并行)
              ↓
Layer 2: Task 7 (index.ts)    Layer 3: Task 3 (tsup.config.ts)  (可并行)
              ↓
Layer 4: Task 9 (验证)
```

## Builder 配置

### 注意事项

1. **ESM 导入路径必须带 `.js` 扩展名**: 在 `src/index.ts` 中导入本地模块时必须使用 `./daemon.js` 而非 `./daemon`，这是 ESM 规范要求，tsup 不会自动添加扩展名
2. **tsup `bundle: false` 模式**: 设置 `bundle: false` 后，tsup 只做 TypeScript 转译而不打包，保持文件结构清晰，适合作为库发布；如果 `bundle: false` 导致 `pnpm build` 找不到模块，改为 `bundle: true` 并设置 `external: []`
3. **pnpm install 必须在 `src/plugin/` 目录内执行**: 不要在 GhostCode 根目录执行，因为根目录是 Cargo workspace，没有 package.json

### 潜在问题及解决方案

| 问题 | 原因 | 解决方案 |
|------|------|---------|
| `Cannot find module './daemon.js'` | 文件未创建或路径错误 | 确认 Task 4 已完成，检查文件路径 |
| `pnpm build: command not found` | pnpm 未安装或未 install | 先执行 `pnpm install` |
| `Type error: ...` | strict 模式下类型不匹配 | 检查占位函数的参数类型是否与接口一致 |
| `Cannot use import statement` | module 字段未设置为 "module" | 检查 package.json 中 `"type": "module"` |
