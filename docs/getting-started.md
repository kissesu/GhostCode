<!-- @file GhostCode 快速上手指南 -->
<!-- @author Atlas.oi -->
<!-- @date 2026-03-04 -->

# GhostCode 快速上手指南

> 作者：Atlas.oi
> 日期：2026-03-04

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

---

## 安装方式

GhostCode 支持三种安装场景，根据你的使用需求选择合适的方式。

### 场景一：在线安装（推荐）

适用于有网络连接的日常开发环境。

```bash
# 第一步：安装 GhostCode npm 包
# postinstall 脚本会自动从 GitHub Release 下载对应平台的 Daemon 二进制
pnpm add ghostcode

# 第二步：初始化当前项目
# 生成 .mcp.json，注册 16 个 MCP 工具，下载并安装 Daemon 二进制到 ~/.ghostcode/bin/
ghostcode init

# 第三步：验证安装
ghostcode doctor
```

安装完成后，`.mcp.json` 会自动生成在项目根目录，内容示例：

```json
{
  "mcpServers": {
    "ghostcode": {
      "command": "node",
      "args": ["./node_modules/ghostcode/dist/index.js"],
      "env": {
        "GHOSTCODE_PROJECT_DIR": "${workspaceFolder}"
      }
    }
  }
}
```

Claude Code 在下次启动时会自动读取 `.mcp.json` 并注册 GhostCode 的 16 个 MCP 工具。

### 场景二：离线安装

适用于网络受限的生产环境或内网环境。

```bash
# 第一步：在有网络的机器上预下载 bundle 和 checksum
# macOS Apple Silicon
curl -L https://github.com/<owner>/ghostcode/releases/latest/download/ghostcode-daemon-aarch64-apple-darwin.tar.gz \
  -o ghostcode-daemon.tar.gz
curl -L https://github.com/<owner>/ghostcode/releases/latest/download/ghostcode-daemon-aarch64-apple-darwin.tar.gz.sha256 \
  -o ghostcode-daemon.tar.gz.sha256

# 第二步：将文件传输到目标机器后，校验 checksum
shasum -a 256 -c ghostcode-daemon.tar.gz.sha256

# 第三步：解压并安装
mkdir -p ~/.ghostcode/bin
tar -xzf ghostcode-daemon.tar.gz -C ~/.ghostcode/bin/
chmod +x ~/.ghostcode/bin/ghostcoded

# 第四步：安装 npm 包（使用离线 tarball）
pnpm add ./ghostcode-<version>.tgz --ignore-scripts

# 第五步：手动执行 postinstall（跳过下载步骤，使用已安装的二进制）
GHOSTCODE_SKIP_DOWNLOAD=1 ghostcode init
```

### 场景三：CI/CD 环境安装

适用于 GitHub Actions、GitLab CI 等自动化流水线。

```yaml
# .github/workflows/ghostcode-setup.yml 示例
- name: 安装 GhostCode
  run: |
    pnpm add ghostcode
    ghostcode init --non-interactive

- name: 验证安装
  run: ghostcode doctor --json | jq '.status'
```

CI 环境中 `ghostcode init --non-interactive` 会跳过交互提示，使用默认配置。

---

## ghostcode init 详解

`ghostcode init` 是安装后必须运行的初始化命令，它会完成以下步骤：

### 执行流程

```
ghostcode init
  |
  +-- 1. 检测平台（macOS/Linux, aarch64/x86_64）
  +-- 2. 下载 Daemon 二进制（如未安装）
  |       从 GitHub Release 下载对应平台 bundle
  |       校验 SHA256 checksum（失败则终止，不降级）
  +-- 3. 安装二进制到 ~/.ghostcode/bin/ghostcoded
  +-- 4. 创建 ~/.ghostcode/.installed 标记文件
  +-- 5. 生成项目 .mcp.json（MCP 工具注册配置）
  +-- 6. 生成项目配置文件 .ghostcode.toml（如不存在）
  +-- 7. 打印安装摘要和后续步骤提示
```

### 初始化后的目录结构

```
~/.ghostcode/
  bin/
    ghostcoded          # Daemon 二进制
  .installed            # 安装标记（内含版本信息）
  config.toml           # 全局配置（global 层）
  groups/               # 各 Group 的账本目录
    <group-id>/
      ledger.jsonl      # 事件账本（NDJSON）
  daemon/
    ghostcoded.addr.json  # Daemon 连接地址（运行时生成）

项目根目录/
  .mcp.json             # MCP 工具注册配置
  .ghostcode.toml       # 项目级配置（project 层，可选）
```

### 配置说明

初始生成的 `.ghostcode.toml` 示例：

```toml
# GhostCode 项目配置
# 更多配置项参见 docs/operations.md

[project]
# 项目名称，用于 group_id 前缀
name = "my-project"

[daemon]
# Daemon socket 路径（默认自动发现）
# socket_path = "/tmp/ghostcoded.sock"

[sovereignty]
# 代码主权约束：禁止非 Claude 写入的文件扩展名
protected_extensions = [".rs", ".ts", ".go", ".py"]
```

---

## 验证安装：ghostcode doctor

`ghostcode doctor` 会执行五类检查并输出诊断报告：

```bash
ghostcode doctor
```

输出示例：

```
GhostCode 诊断报告
==================

[OK] 二进制安装    ~/.ghostcode/bin/ghostcoded v0.5.0
[OK] checksum     SHA256 校验通过
[OK] Daemon 连接  /tmp/ghostcoded.sock (pid: 12345)
[OK] 配置文件     ~/.ghostcode/config.toml + .ghostcode.toml
[OK] MCP 注册     .mcp.json 存在，包含 16 个工具

诊断通过，GhostCode 运行正常。
```

如果某项检查失败，doctor 会给出具体的修复建议。

```bash
# 输出 JSON 格式（适合 CI 解析）
ghostcode doctor --json

# 自动修复可修复的问题
ghostcode doctor --fix
```

---

## 首次使用

### 连接到 Daemon

Plugin 启动时会自动发现并连接 Daemon。如需手动验证：

```bash
# 检查 Daemon 是否运行
cat ~/.ghostcode/daemon/ghostcoded.addr.json

# 通过 health endpoint 验证
curl http://localhost:7070/health
# 预期输出：{"status":"ready","version":"0.5.0"}

# 或通过 Unix Socket 发送 ping
echo '{"op":"ping","params":{}}' | nc -U /tmp/ghostcoded.sock
# 预期输出：{"ok":true,"data":"pong"}
```

### 发送第一条消息

在 Claude Code 中，GhostCode Plugin 已通过 Hook 自动集成。尝试以下 Magic Keywords：

```
ralph
```

如果 Plugin 正常工作，系统会自动激活 Ralph 验证模式。

支持的 Magic Keywords：

| 关键词 | 说明 |
|--------|------|
| `ralph` | Ralph 验证模式 - 代码变更经过 7 项自动验证 |
| `autopilot` | Autopilot 模式 - 全自动执行模式 |
| `team` | Team 模式 - 多 Agent 协作模式 |
| `ultrawork` | UltraWork 模式 - 极致工作模式 |
| `cancel` | 取消当前激活的模式 |

### 打开 Web Dashboard

方式一：在 Claude Code 中输入

```
/gc-web
```

方式二：直接在浏览器中访问

```
http://localhost:7070
```

Dashboard 功能：
- **事件时间轴** - 实时展示账本中的所有事件，通过 SSE 自动更新
- **Agent 状态面板** - 显示所有 Actor 的当前状态（active / stopped）
- **Skill 候选面板** - 展示 Skill Learning 引擎提取的待确认模式

---

## 从源码构建（开发者）

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

### 第四步：本地注册 Plugin

```bash
# 开发模式：手动配置 .mcp.json
cat > .mcp.json << 'EOF'
{
  "mcpServers": {
    "ghostcode": {
      "command": "node",
      "args": ["./src/plugin/dist/index.js"]
    }
  }
}
EOF
```

---

## 常见问题

### Daemon 启动失败

检查是否有残留的 Socket 文件（僵尸进程清理）：

```bash
# 自动诊断并修复
ghostcode doctor --fix

# 手动清理
rm -f /tmp/ghostcoded.sock
./target/release/ghostcoded
```

### Plugin 无法连接到 Daemon

确认 Daemon 已启动，并检查 addr.json 是否存在：

```bash
cat ~/.ghostcode/daemon/ghostcoded.addr.json
```

如果文件不存在，说明 Daemon 未成功启动。使用 `ghostcode doctor` 查看详细原因。

### checksum 校验失败

不要降级或跳过 checksum！这是安全保障：

```bash
# 重新下载（可能是网络问题导致文件损坏）
ghostcode init --force-download
```

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

---

## 下一步

- 阅读 [系统架构说明](architecture.md) 深入了解 GhostCode 的设计
- 阅读 [运维手册](operations.md) 了解配置优先级和故障恢复
- 阅读 [Skill 编写指南](skill-guide.md) 了解如何开发自定义技能
- 查看 `crates/` 目录下各 crate 的源码注释了解实现细节
