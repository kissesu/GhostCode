# GhostCode 快速上手指南

> 作者：Atlas.oi
> 日期：2026-03-03

## 环境要求

在开始之前，确保你的系统已安装以下工具：

| 工具 | 最低版本 | 安装方式 |
|------|---------|---------|
| Rust | 1.75+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js | 18+ | 推荐使用 [volta](https://volta.sh/)：`curl https://get.volta.sh \| bash` |
| pnpm | 8+ | `volta install pnpm` 或 `npm install -g pnpm` |
| Claude Code | 最新版 | 参考 Anthropic 官方文档 |

**支持的操作系统**：
- macOS (Apple Silicon / Intel)
- Linux (x86_64)

## 从源码构建

### 第一步：克隆仓库

```bash
git clone <repository-url> GhostCode
cd GhostCode
```

### 第二步：构建 Rust 核心

```bash
# 开发构建（包含调试信息）
cargo build

# 或发布构建（优化后，推荐生产使用）
cargo build --release
```

构建成功后，Daemon 二进制位于：
- 开发构建：`target/debug/ghostcoded`
- 发布构建：`target/release/ghostcoded`

验证构建结果：

```bash
./target/release/ghostcoded --version
```

### 第三步：构建 Plugin

```bash
cd src/plugin
pnpm install
pnpm build
```

构建产物位于 `src/plugin/dist/`。

## 启动 Daemon

GhostCode Daemon 负责管理所有 Actor 的生命周期和通信。通常情况下，当你安装 Plugin 后，Daemon 会由 Plugin 自动管理（启动和停止）。

如需手动启动（调试用）：

```bash
# 启动 Daemon（前台运行，日志输出到控制台）
./target/release/ghostcoded

# 启动后，Daemon 会在以下路径写入连接信息：
# ~/.ghostcode/daemon/ghostcoded.addr.json
```

`ghostcoded.addr.json` 示例内容：

```json
{
  "v": 1,
  "transport": "unix",
  "path": "/tmp/ghostcoded.sock",
  "pid": 12345,
  "version": "0.1.0"
}
```

验证 Daemon 是否正常运行：

```bash
# 通过 Unix Socket 发送 ping 请求（需要安装 jq）
echo '{"op":"ping","params":{}}' | nc -U /tmp/ghostcoded.sock
# 预期输出：{"ok":true,"data":"pong"}
```

## 安装 Plugin

### 方式一：通过 Claude Code Plugin 系统（推荐）

1. 确保 Claude Code 已安装并配置完成
2. 将构建好的 Plugin 注册到 Claude Code：

```bash
# 在项目目录中安装 GhostCode Plugin
claude plugin install ./src/plugin/dist

# 或者全局安装
claude plugin install --global ./src/plugin/dist
```

3. Plugin 首次启动时会自动：
   - 将 `ghostcoded` 二进制复制到 `~/.ghostcode/bin/ghostcoded`
   - 创建 `~/.ghostcode/.installed` 标记文件
   - 在需要时自动启动 Daemon

### 方式二：手动配置（开发调试）

在项目的 `.claude/settings.json` 中添加 Plugin 配置：

```json
{
  "plugins": [
    {
      "name": "ghostcode",
      "path": "./src/plugin/dist/index.js"
    }
  ]
}
```

## 启动 Web Dashboard

Web Dashboard 提供实时监控界面，展示事件时间轴、Agent 状态和 Skill 候选。

### 第一步：确保 Daemon 正在运行

Dashboard 通过读取账本文件获取数据，需要 Daemon 先运行并产生事件。

### 第二步：启动 Web 服务器

```bash
# 启动 ghostcode-web 服务器
cargo run -p ghostcode-web

# 或使用发布构建
./target/release/ghostcode-web
```

服务器默认绑定 `127.0.0.1:7070`。

### 第三步：打开 Dashboard

在浏览器中访问：`http://localhost:7070`

Dashboard 功能：
- **事件时间轴** - 实时展示账本中的所有事件，通过 SSE 自动更新
- **Agent 状态面板** - 显示所有 Actor 的当前状态（active / stopped）
- **Skill 候选面板** - 展示 Skill Learning 引擎提取的待确认模式

## 验证安装

完成安装后，通过以下步骤验证 GhostCode 是否正常工作：

### 1. 检查 Daemon 状态

```bash
# 检查 addr.json 是否存在
ls -la ~/.ghostcode/daemon/ghostcoded.addr.json

# 检查 Daemon 进程是否运行
ps aux | grep ghostcoded
```

### 2. 测试 Magic Keywords

在 Claude Code 中输入以下内容，验证 Plugin 是否正常工作：

```
ralph
```

如果 Plugin 正常工作，Claude 的回复上下文中应包含：

```
[GhostCode] Ralph 验证模式已激活 - 代码变更将经过 7 项自动验证
```

支持的 Magic Keywords：

| 关键词 | 说明 |
|--------|------|
| `ralph` | Ralph 验证模式 - 代码变更经过 7 项自动验证 |
| `autopilot` | Autopilot 模式 - 全自动执行模式 |
| `team` | Team 模式 - 多 Agent 协作模式 |
| `ultrawork` | UltraWork 模式 - 极致工作模式 |
| `cancel` | 取消当前激活的模式 |

### 3. 验证账本写入

```bash
# 查看账本文件（位置取决于 group_id 配置）
ls ~/.ghostcode/groups/

# 查看最近 10 条事件
tail -10 ~/.ghostcode/groups/<group-id>/ledger.jsonl
```

### 4. 访问 Dashboard API

```bash
# 获取 Dashboard 快照
curl http://localhost:7070/dashboard

# 获取最近事件时间线
curl "http://localhost:7070/timeline?limit=20"

# 查看 Agent 状态
curl http://localhost:7070/agents

# 订阅实时事件流（SSE）
curl -N http://localhost:7070/events
```

## 常见问题

### Daemon 启动失败

检查端口是否被占用，或是否有残留的 Socket 文件：

```bash
# 删除残留的 Socket 文件后重试
rm -f /tmp/ghostcoded.sock
./target/release/ghostcoded
```

### Plugin 无法连接到 Daemon

确认 Daemon 已启动，并检查 addr.json 是否存在：

```bash
cat ~/.ghostcode/daemon/ghostcoded.addr.json
```

如果文件不存在，说明 Daemon 未成功启动。查看 Daemon 日志排查原因。

### 构建失败（Rust）

确认 Rust 版本满足要求（1.75+）：

```bash
rustc --version
rustup update stable
```

### 构建失败（TypeScript）

确认 Node.js 版本满足要求（18+）：

```bash
node --version
pnpm --version
```

## 下一步

- 阅读 [系统架构说明](architecture.md) 深入了解 GhostCode 的设计
- 查看 `crates/` 目录下各 crate 的源码注释了解实现细节
- 参与贡献：查看 README.md 中的开发指南
