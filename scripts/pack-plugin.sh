#!/usr/bin/env bash
# scripts/pack-plugin.sh — GhostCode Plugin 打包脚本
#
# 将三平台 Rust 二进制 + TypeScript 编译产物 + 配置文件
# 组装为符合 Claude Code Plugin 规范的 npm 包目录。
#
# 架构拓扑说明（混合架构）：
#   - Node.js Extension（dist/index.js）：处理 Claude Code Hooks，
#     负责 Magic Keywords、HUD 状态栏、用户交互层等 Plugin 功能
#   - Rust MCP Server（bin/ghostcode-mcp）：通过 stdio JSON-RPC 2.0
#     与 Claude Code 通信，提供多 Agent 协作工具能力，连接 Daemon
#   - Rust Daemon（bin/ghostcoded-*）：后台守护进程，管理 Agent 生命周期、
#     消息投递、Session Lease 等核心引擎功能
#
# 启动拓扑：
#   Claude Code
#     ├── Extension (Node.js)  →  dist/index.js         [Hooks 处理]
#     └── MCP Server (Rust)    →  bin/ghostcode-mcp     [工具能力]
#                                      ↓ Unix Socket
#                              Daemon (bin/ghostcoded-*)  [核心引擎]
#
# 用法:
#   bash scripts/pack-plugin.sh \
#     --binaries-dir <path>   二进制文件所在目录（含 ghostcode-mcp 和 ghostcoded-* 二进制）
#     --ts-dist <path>        TypeScript 编译产物目录（tsup 输出）
#     --version <semver>      版本号（如 0.1.0，不含 v 前缀）
#     --output <path>         输出目录（默认 ghostcode-plugin/）
#
# 预期输出结构:
#   <output>/
#     package.json
#     dist/
#       index.js
#     bin/
#       ghostcode-mcp              # MCP Server（Rust，stdio JSON-RPC）
#       ghostcoded-darwin-arm64    # Daemon（macOS Apple Silicon）
#       ghostcoded-darwin-x64      # Daemon（macOS Intel）
#       ghostcoded-linux-x64       # Daemon（Linux x86_64）
#     .claude/
#       settings.json
#
# @author Atlas.oi
# @date 2026-03-04

set -euo pipefail

# ============================================
# 参数解析
# ============================================
BINARIES_DIR=""
TS_DIST=""
VERSION=""
OUTPUT_DIR="ghostcode-plugin"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --binaries-dir) BINARIES_DIR="$2"; shift 2 ;;
    --ts-dist)      TS_DIST="$2";      shift 2 ;;
    --version)      VERSION="$2";      shift 2 ;;
    --output)       OUTPUT_DIR="$2";   shift 2 ;;
    *) echo "未知参数: $1" >&2; exit 1 ;;
  esac
done

# ============================================
# 参数验证
# ============================================
if [[ -z "$BINARIES_DIR" || -z "$TS_DIST" || -z "$VERSION" ]]; then
  echo "用法: $0 --binaries-dir <path> --ts-dist <path> --version <semver> [--output <path>]" >&2
  exit 1
fi

if [[ ! -d "$BINARIES_DIR" ]]; then
  echo "错误: 二进制目录不存在: $BINARIES_DIR" >&2
  exit 1
fi

if [[ ! -d "$TS_DIST" ]]; then
  echo "错误: TypeScript 产物目录不存在: $TS_DIST" >&2
  exit 1
fi

echo "开始打包 GhostCode Plugin v${VERSION}"
echo "  二进制目录: $BINARIES_DIR"
echo "  TS 产物目录: $TS_DIST"
echo "  输出目录: $OUTPUT_DIR"

# ============================================
# 第一步：清理并创建输出目录结构
# ============================================
echo ""
echo "第一步：创建目录结构..."
rm -rf "$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR/dist"
mkdir -p "$OUTPUT_DIR/bin"
mkdir -p "$OUTPUT_DIR/.claude"

# ============================================
# 第二步：复制 TypeScript 编译产物
# ============================================
echo "第二步：复制 TypeScript 产物..."

# 验证三个必须的 JS 入口文件存在
for entry in index.js cli.js postinstall.js; do
  if [[ ! -f "$TS_DIST/$entry" ]]; then
    echo "错误: 缺少 TypeScript 产物: $TS_DIST/$entry" >&2
    echo "      请先运行 pnpm --dir src/plugin run build" >&2
    exit 1
  fi
done

# 复制三个入口文件
cp "$TS_DIST/index.js" "$OUTPUT_DIR/dist/index.js"
cp "$TS_DIST/cli.js" "$OUTPUT_DIR/dist/cli.js"
cp "$TS_DIST/postinstall.js" "$OUTPUT_DIR/dist/postinstall.js"
echo "  复制: dist/index.js, dist/cli.js, dist/postinstall.js"

# 复制类型声明（如存在）
for dts in "$TS_DIST"/*.d.ts; do
  if [[ -f "$dts" ]]; then
    cp "$dts" "$OUTPUT_DIR/dist/"
  fi
done

# ============================================
# 第三步：复制二进制文件
# 包含两类二进制：
#   1. ghostcode-mcp — MCP Server，通过 stdio 与 Claude Code 通信
#      作为 MCP server 注册到 .claude/settings.json
#   2. ghostcoded-* — 三平台 Daemon，被 ghostcode-mcp 通过 Unix Socket 调用
# 每个二进制复制后设置可执行权限
# ============================================
echo "第三步：复制二进制文件..."

# MCP Server 二进制（单一跨平台版本，作为 settings.json 的 MCP server command）
MCP_SERVER_BIN="ghostcode-mcp"
MCP_SERVER_SRC="$BINARIES_DIR/$MCP_SERVER_BIN"
MCP_SERVER_DST="$OUTPUT_DIR/bin/$MCP_SERVER_BIN"

if [[ ! -f "$MCP_SERVER_SRC" ]]; then
  echo "错误: 缺少 MCP Server 二进制: $MCP_SERVER_SRC" >&2
  exit 1
fi

cp "$MCP_SERVER_SRC" "$MCP_SERVER_DST"
chmod +x "$MCP_SERVER_DST"
echo "  复制: bin/$MCP_SERVER_BIN (MCP Server)"

# Daemon 三平台二进制（后台守护进程，管理 Agent 生命周期）
DAEMON_BINARIES=(
  "ghostcoded-darwin-arm64"
  "ghostcoded-darwin-x64"
  "ghostcoded-linux-x64"
)

for bin in "${DAEMON_BINARIES[@]}"; do
  src="$BINARIES_DIR/$bin"
  dst="$OUTPUT_DIR/bin/$bin"

  if [[ ! -f "$src" ]]; then
    echo "错误: 缺少 Daemon 二进制文件: $src" >&2
    exit 1
  fi

  cp "$src" "$dst"
  chmod +x "$dst"
  echo "  复制: bin/$bin (Daemon)"
done

# ============================================
# 第四步：生成 package.json
# 使用传入的版本号填充
# ============================================
echo "第四步：生成 package.json..."

cat > "$OUTPUT_DIR/package.json" << EOF
{
  "name": "@ghostcode/plugin",
  "version": "${VERSION}",
  "description": "GhostCode - 多 Agent 协作开发平台 Claude Code Plugin",
  "type": "module",
  "main": "dist/index.js",
  "bin": {
    "ghostcode": "dist/cli.js"
  },
  "scripts": {
    "postinstall": "node dist/postinstall.js"
  },
  "files": [
    "dist",
    "bin",
    ".claude"
  ],
  "engines": {
    "node": ">=20"
  },
  "keywords": [
    "claude",
    "claude-code",
    "plugin",
    "multi-agent",
    "ghostcode"
  ],
  "author": "Atlas.oi",
  "license": "MIT",
  "repository": {
    "type": "git",
    "url": "https://github.com/kissesu/GhostCode.git"
  }
}
EOF
echo "  生成: package.json"

# ============================================
# 第五步：生成 .claude/settings.json
# Claude Code Plugin 配置文件
#
# 混合架构配置说明：
#   - mcpServers.ghostcode：注册 Rust MCP Server 二进制
#     command 指向 bin/ghostcode-mcp（相对于插件安装目录）
#     该二进制通过 stdio 实现 JSON-RPC 2.0 与 Claude Code 通信
#     内部通过 Unix Socket 连接后台 Daemon
#
#   注意：Node.js Extension（dist/index.js）作为 Claude Code Plugin
#   的 extension 入口自动加载，无需在 settings.json 中单独注册，
#   其职责为 Hooks 处理（Magic Keywords、HUD 状态栏等 UI 层功能）
# ============================================
echo "第五步：生成 .claude/settings.json..."

cat > "$OUTPUT_DIR/.claude/settings.json" << EOF
{
  "mcpServers": {
    "ghostcode": {
      "command": "./bin/ghostcode-mcp",
      "args": ["--base-dir", "\${HOME}/.ghostcode"],
      "description": "GhostCode 多 Agent 协作开发平台 MCP Server"
    }
  }
}
EOF
echo "  生成: .claude/settings.json"

# ============================================
# 第六步：验证最终包结构
# ============================================
echo "第六步：验证包结构..."

REQUIRED_FILES=(
  "package.json"
  "dist/index.js"
  "bin/ghostcode-mcp"
  "bin/ghostcoded-darwin-arm64"
  "bin/ghostcoded-darwin-x64"
  "bin/ghostcoded-linux-x64"
  ".claude/settings.json"
)

ALL_OK=true
for f in "${REQUIRED_FILES[@]}"; do
  if [[ -f "$OUTPUT_DIR/$f" ]]; then
    echo "  OK: $f"
  else
    echo "  缺失: $f" >&2
    ALL_OK=false
  fi
done

if [[ "$ALL_OK" != "true" ]]; then
  echo "" >&2
  echo "错误: Plugin 包结构验证失败" >&2
  exit 1
fi

echo ""
echo "打包完成: $OUTPUT_DIR/"
echo "包大小："
du -sh "$OUTPUT_DIR/"
