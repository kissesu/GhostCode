#!/usr/bin/env bash
# scripts/pack-plugin.sh — GhostCode Plugin 打包脚本
#
# 将三平台 Rust 二进制 + TypeScript 编译产物 + Plugin 配置文件
# 组装为符合 Claude Code Plugin 规范的 npm 包目录。
#
# 架构拓扑说明（混合架构）：
#   - Node.js Extension（dist/index.js）：被 scripts/*.mjs 动态 import，
#     处理 Claude Code Hooks（Magic Keywords、HUD 状态栏、用户交互层等）
#   - Rust MCP Server（bin/ghostcode-mcp）：通过 stdio JSON-RPC 2.0
#     与 Claude Code 通信，提供多 Agent 协作工具能力，连接 Daemon
#   - Rust Daemon（bin/ghostcoded-*）：后台守护进程，管理 Agent 生命周期、
#     消息投递、Session Lease 等核心引擎功能
#
# 启动拓扑：
#   Claude Code
#     ├── Extension (Node.js)  →  scripts/hook-*.mjs    [Hooks 处理]
#     │                               ↓ dynamic import
#     │                          dist/index.js           [核心模块]
#     └── MCP Server (Rust)    →  bin/ghostcode-mcp     [工具能力]
#                                      ↓ Unix Socket
#                              Daemon (bin/ghostcoded-*)  [核心引擎]
#
# 用法:
#   bash scripts/pack-plugin.sh \
#     --binaries-dir <path>   二进制文件所在目录（含 ghostcode-mcp 和 ghostcoded-* 二进制）
#     --ts-dist <path>        TypeScript 编译产物目录（tsup 输出）
#     --version <semver>      版本号（如 0.1.0，不含 v 前缀）
#     --plugin-src <path>     Plugin 源目录（默认 src/plugin）
#     --output <path>         输出目录（默认 ghostcode-plugin/）
#
# 预期输出结构:
#   <output>/
#     package.json
#     .claude-plugin/
#       plugin.json
#     hooks/
#       hooks.json
#     skills/
#       research/SKILL.md
#       plan/SKILL.md
#       execute/SKILL.md
#       review/SKILL.md
#     .mcp.json
#     scripts/
#       run.mjs
#       hook-pre-tool-use.mjs
#       hook-stop.mjs
#       hook-user-prompt-submit.mjs
#       hook-session-start.mjs
#       hook-session-end.mjs
#       hook-subagent-start.mjs
#       hook-subagent-stop.mjs
#       hook-pre-compact.mjs
#       lib/
#         daemon-client.mjs
#         stdin.mjs
#     prompts/
#       codex-analyzer.md
#       gemini-analyzer.md
#       codex-reviewer.md
#       gemini-reviewer.md
#     dist/
#       index.js       (核心模块，被 scripts/*.mjs 动态 import)
#       cli.js         (CLI 工具)
#     bin/
#       ghostcode-mcp              (平台检测启动器)
#       ghostcode-mcp-darwin-arm64
#       ghostcode-mcp-darwin-x64
#       ghostcode-mcp-linux-x64
#       ghostcode-wrapper              (平台检测启动器)
#       ghostcode-wrapper-darwin-arm64
#       ghostcode-wrapper-darwin-x64
#       ghostcode-wrapper-linux-x64
#       ghostcoded-darwin-arm64
#       ghostcoded-darwin-x64
#       ghostcoded-linux-x64
#
# @author Atlas.oi
# @date 2026-03-05

set -euo pipefail

# ============================================
# 参数解析
# ============================================
BINARIES_DIR=""
TS_DIST=""
VERSION=""
OUTPUT_DIR="ghostcode-plugin"
PLUGIN_SRC="src/plugin"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --binaries-dir) BINARIES_DIR="$2"; shift 2 ;;
    --ts-dist)      TS_DIST="$2";      shift 2 ;;
    --version)      VERSION="$2";      shift 2 ;;
    --output)       OUTPUT_DIR="$2";   shift 2 ;;
    --plugin-src)   PLUGIN_SRC="$2";   shift 2 ;;
    *) echo "未知参数: $1" >&2; exit 1 ;;
  esac
done

# ============================================
# 参数验证
# ============================================
if [[ -z "$BINARIES_DIR" || -z "$TS_DIST" || -z "$VERSION" ]]; then
  echo "用法: $0 --binaries-dir <path> --ts-dist <path> --version <semver> [--plugin-src <path>] [--output <path>]" >&2
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

if [[ ! -d "$PLUGIN_SRC" ]]; then
  echo "错误: Plugin 源目录不存在: $PLUGIN_SRC" >&2
  exit 1
fi

echo "开始打包 GhostCode Plugin v${VERSION}"
echo "  二进制目录: $BINARIES_DIR"
echo "  TS 产物目录: $TS_DIST"
echo "  Plugin 源目录: $PLUGIN_SRC"
echo "  输出目录: $OUTPUT_DIR"

# ============================================
# 第一步：清理并创建输出目录结构
# ============================================
echo ""
echo "第一步：创建目录结构..."
rm -rf "$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR/dist"
mkdir -p "$OUTPUT_DIR/bin"
mkdir -p "$OUTPUT_DIR/.claude-plugin"
mkdir -p "$OUTPUT_DIR/hooks"
mkdir -p "$OUTPUT_DIR/skills"
mkdir -p "$OUTPUT_DIR/scripts"

# ============================================
# 第二步：复制 TypeScript 编译产物
# 只需要 index.js（核心模块）和 cli.js（CLI 工具）
# postinstall.js 在新架构中不再需要
# ============================================
echo "第二步：复制 TypeScript 产物..."

# 验证必须的 JS 入口文件存在
for entry in index.js cli.js; do
  if [[ ! -f "$TS_DIST/$entry" ]]; then
    echo "错误: 缺少 TypeScript 产物: $TS_DIST/$entry" >&2
    echo "      请先运行 pnpm --dir src/plugin run build" >&2
    exit 1
  fi
done

# 复制两个入口文件
cp "$TS_DIST/index.js" "$OUTPUT_DIR/dist/index.js"
cp "$TS_DIST/cli.js" "$OUTPUT_DIR/dist/cli.js"
echo "  复制: dist/index.js, dist/cli.js"

# 复制类型声明（如存在）
for dts in "$TS_DIST"/*.d.ts; do
  if [[ -f "$dts" ]]; then
    cp "$dts" "$OUTPUT_DIR/dist/"
  fi
done

# ============================================
# 第三步：复制 Plugin 配置文件
# 包括 plugin.json、hooks.json、.mcp.json、skills、scripts
# 这些文件定义了 Plugin 的能力和 Hook 处理脚本
# ============================================
echo "第三步：复制 Plugin 配置文件..."

# 复制 .claude-plugin/plugin.json（Plugin 元数据配置）
if [[ ! -f "$PLUGIN_SRC/.claude-plugin/plugin.json" ]]; then
  echo "错误: 缺少 Plugin 配置文件: $PLUGIN_SRC/.claude-plugin/plugin.json" >&2
  exit 1
fi
cp "$PLUGIN_SRC/.claude-plugin/plugin.json" "$OUTPUT_DIR/.claude-plugin/plugin.json"
echo "  复制: .claude-plugin/plugin.json"

# 复制 hooks/hooks.json（Hook 事件注册配置）
if [[ ! -f "$PLUGIN_SRC/hooks/hooks.json" ]]; then
  echo "错误: 缺少 Hooks 配置文件: $PLUGIN_SRC/hooks/hooks.json" >&2
  exit 1
fi
cp "$PLUGIN_SRC/hooks/hooks.json" "$OUTPUT_DIR/hooks/hooks.json"
echo "  复制: hooks/hooks.json"

# 复制 .mcp.json（MCP Server 注册配置）
if [[ ! -f "$PLUGIN_SRC/.mcp.json" ]]; then
  echo "错误: 缺少 MCP 配置文件: $PLUGIN_SRC/.mcp.json" >&2
  exit 1
fi
cp "$PLUGIN_SRC/.mcp.json" "$OUTPUT_DIR/.mcp.json"
echo "  复制: .mcp.json"

# 复制 Skills 目录（每个 skill 的 SKILL.md 定义文件）
if [[ -d "$PLUGIN_SRC/skills" ]]; then
  for skill_dir in "$PLUGIN_SRC/skills"/*/; do
    if [[ -d "$skill_dir" ]]; then
      skill_name=$(basename "$skill_dir")
      mkdir -p "$OUTPUT_DIR/skills/$skill_name"
      if [[ -f "$skill_dir/SKILL.md" ]]; then
        cp "$skill_dir/SKILL.md" "$OUTPUT_DIR/skills/$skill_name/SKILL.md"
        echo "  复制: skills/$skill_name/SKILL.md"
      else
        echo "警告: skills/$skill_name 目录下缺少 SKILL.md" >&2
      fi
    fi
  done
else
  echo "警告: Plugin 源目录下不存在 skills/ 目录，跳过 Skills 复制" >&2
fi

# 复制 Scripts（Hook 处理脚本，被 hooks.json 引用）
# 复制全部 .mjs 脚本（排除 install-wrapper.mjs，该脚本仅用于本地开发）
for script in "$PLUGIN_SRC/scripts/"*.mjs; do
  if [[ -f "$script" ]]; then
    script_name=$(basename "$script")
    # install-wrapper.mjs 是开发用脚本，不纳入分发
    if [[ "$script_name" == "install-wrapper.mjs" ]]; then
      echo "  跳过: scripts/$script_name (开发用，不分发)"
      continue
    fi
    cp "$script" "$OUTPUT_DIR/scripts/$script_name"
    echo "  复制: scripts/$script_name"
  fi
done

# 复制 lib 目录（daemon-client.mjs, stdin.mjs 等共享模块）
if [[ -d "$PLUGIN_SRC/scripts/lib" ]]; then
  mkdir -p "$OUTPUT_DIR/scripts/lib"
  for lib_file in "$PLUGIN_SRC/scripts/lib/"*.mjs; do
    if [[ -f "$lib_file" ]]; then
      cp "$lib_file" "$OUTPUT_DIR/scripts/lib/$(basename "$lib_file")"
      echo "  复制: scripts/lib/$(basename "$lib_file")"
    fi
  done
else
  echo "警告: Plugin 源目录下不存在 scripts/lib/ 目录" >&2
fi

# 复制 Prompts 目录（多模型协作的角色提示词文件）
# W3-review：改为动态扫描 *.md 文件，新增 prompt 文件时无需手动更新此处
echo "  复制 Prompts 目录..."
mkdir -p "$OUTPUT_DIR/prompts"
PROMPT_COUNT=0
if [[ -d "$PLUGIN_SRC/prompts" ]]; then
  for prompt in "$PLUGIN_SRC/prompts/"*.md; do
    if [[ -f "$prompt" ]]; then
      prompt_name=$(basename "$prompt")
      cp "$prompt" "$OUTPUT_DIR/prompts/$prompt_name"
      echo "  复制: prompts/$prompt_name"
      PROMPT_COUNT=$((PROMPT_COUNT + 1))
    fi
  done
fi
if [[ "$PROMPT_COUNT" -eq 0 ]]; then
  echo "错误: prompts 目录为空或不存在: $PLUGIN_SRC/prompts/" >&2
  exit 1
fi
echo "  共复制 $PROMPT_COUNT 个角色提示词文件"

# ============================================
# 第四步：复制二进制文件
# 包含两类二进制：
#   1. ghostcode-mcp — MCP Server，通过 stdio 与 Claude Code 通信
#      注册在 .mcp.json 中，替代旧版 .claude/settings.json
#   2. ghostcoded-* — 三平台 Daemon，被 ghostcode-mcp 通过 Unix Socket 调用
# 每个二进制复制后设置可执行权限
# ============================================
echo "第四步：复制二进制文件..."

# MCP Server 三平台二进制（通过 stdio 与 Claude Code 通信）
MCP_BINARIES=(
  "ghostcode-mcp-darwin-arm64"
  "ghostcode-mcp-darwin-x64"
  "ghostcode-mcp-linux-x64"
)

for bin in "${MCP_BINARIES[@]}"; do
  src="$BINARIES_DIR/$bin"
  dst="$OUTPUT_DIR/bin/$bin"

  if [[ ! -f "$src" ]]; then
    echo "错误: 缺少 MCP Server 二进制文件: $src" >&2
    exit 1
  fi

  cp "$src" "$dst"
  chmod +x "$dst"
  echo "  复制: bin/$bin (MCP Server)"
done

# 生成 MCP Server 平台检测启动器脚本
# .mcp.json 通过此脚本调用 MCP Server，
# 脚本自动检测运行平台并 exec 对应的二进制
cat > "$OUTPUT_DIR/bin/ghostcode-mcp" << 'LAUNCHER_EOF'
#!/bin/sh
# ghostcode-mcp 平台检测启动器
# 自动选择当前平台对应的 MCP Server 二进制并执行
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PLATFORM="$(uname -s)-$(uname -m)"
case "$PLATFORM" in
  Darwin-arm64)  exec "$SCRIPT_DIR/ghostcode-mcp-darwin-arm64" "$@" ;;
  Darwin-x86_64) exec "$SCRIPT_DIR/ghostcode-mcp-darwin-x64" "$@" ;;
  Linux-x86_64)  exec "$SCRIPT_DIR/ghostcode-mcp-linux-x64" "$@" ;;
  *) echo "[GhostCode] 不支持的平台: $PLATFORM" >&2; exit 1 ;;
esac
LAUNCHER_EOF
chmod +x "$OUTPUT_DIR/bin/ghostcode-mcp"
echo "  生成: bin/ghostcode-mcp (平台检测启动器)"

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

# Wrapper 三平台二进制（多模型协作的 AI 后端统一调用 CLI 工具）
WRAPPER_BINARIES=(
  "ghostcode-wrapper-darwin-arm64"
  "ghostcode-wrapper-darwin-x64"
  "ghostcode-wrapper-linux-x64"
)

for bin in "${WRAPPER_BINARIES[@]}"; do
  src="$BINARIES_DIR/$bin"
  dst="$OUTPUT_DIR/bin/$bin"

  if [[ ! -f "$src" ]]; then
    echo "错误: 缺少 Wrapper 二进制文件: $src" >&2
    exit 1
  fi

  cp "$src" "$dst"
  chmod +x "$dst"
  echo "  复制: bin/$bin (Wrapper)"
done

# 生成 Wrapper 平台检测启动器脚本
# SKILL.md 中通过 ~/.ghostcode/bin/ghostcode-wrapper 调用，
# session-start hook 会将此启动器 symlink 到用户目录
cat > "$OUTPUT_DIR/bin/ghostcode-wrapper" << 'LAUNCHER_EOF'
#!/bin/sh
# ghostcode-wrapper 平台检测启动器
# 自动选择当前平台对应的 Wrapper 二进制并执行
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PLATFORM="$(uname -s)-$(uname -m)"
case "$PLATFORM" in
  Darwin-arm64)  exec "$SCRIPT_DIR/ghostcode-wrapper-darwin-arm64" "$@" ;;
  Darwin-x86_64) exec "$SCRIPT_DIR/ghostcode-wrapper-darwin-x64" "$@" ;;
  Linux-x86_64)  exec "$SCRIPT_DIR/ghostcode-wrapper-linux-x64" "$@" ;;
  *) echo "[GhostCode] 不支持的平台: $PLATFORM" >&2; exit 1 ;;
esac
LAUNCHER_EOF
chmod +x "$OUTPUT_DIR/bin/ghostcode-wrapper"
echo "  生成: bin/ghostcode-wrapper (平台检测启动器)"

# ============================================
# 第五步：生成 package.json
# 新架构移除了 main/exports 字段（dist/index.js 由 scripts/*.mjs 动态 import）
# 移除了 postinstall script（不再需要安装后注册逻辑）
# ============================================
echo "第五步：生成 package.json..."

cat > "$OUTPUT_DIR/package.json" << EOF
{
  "name": "@atlas-ghost/ghostcode",
  "version": "${VERSION}",
  "description": "GhostCode - 多 Agent 协作开发平台 Claude Code Plugin",
  "type": "module",
  "bin": {
    "ghostcode": "dist/cli.js"
  },
  "files": [
    "dist",
    ".claude-plugin",
    "hooks",
    "skills",
    ".mcp.json",
    "scripts",
    "bin",
    "prompts"
  ],
  "engines": {
    "node": ">=20"
  },
  "keywords": ["claude", "claude-code", "plugin", "multi-agent", "ghostcode"],
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
# 第六步：验证最终包结构
# 确保所有必须文件都已正确复制/生成
# ============================================
echo "第六步：验证包结构..."

REQUIRED_FILES=(
  "package.json"
  ".claude-plugin/plugin.json"
  "hooks/hooks.json"
  ".mcp.json"
  # Hook 脚本
  "scripts/run.mjs"
  "scripts/hook-pre-tool-use.mjs"
  "scripts/hook-stop.mjs"
  "scripts/hook-user-prompt-submit.mjs"
  "scripts/hook-session-start.mjs"
  "scripts/hook-session-end.mjs"
  "scripts/hook-subagent-start.mjs"
  "scripts/hook-subagent-stop.mjs"
  "scripts/hook-pre-compact.mjs"
  "scripts/lib/daemon-client.mjs"
  "scripts/lib/stdin.mjs"
  # TS 产物
  "dist/index.js"
  "dist/cli.js"
  # MCP Server 二进制
  "bin/ghostcode-mcp"
  "bin/ghostcode-mcp-darwin-arm64"
  "bin/ghostcode-mcp-darwin-x64"
  "bin/ghostcode-mcp-linux-x64"
  # Daemon 二进制
  "bin/ghostcoded-darwin-arm64"
  "bin/ghostcoded-darwin-x64"
  "bin/ghostcoded-linux-x64"
  # Wrapper 二进制
  "bin/ghostcode-wrapper"
  "bin/ghostcode-wrapper-darwin-arm64"
  "bin/ghostcode-wrapper-darwin-x64"
  "bin/ghostcode-wrapper-linux-x64"
  # Prompts 目录：W3-review 改为动态扫描，此处仅验证目录存在
  # 实际文件数已在第三步动态扫描时验证（PROMPT_COUNT > 0）
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

# W3-review：验证 Prompts 目录非空（动态扫描，不依赖硬编码列表）
PROMPT_FILES_COUNT=$(ls "$OUTPUT_DIR/prompts/"*.md 2>/dev/null | wc -l | tr -d ' ')
if [[ "$PROMPT_FILES_COUNT" -gt 0 ]]; then
  echo "  OK: prompts/ ($PROMPT_FILES_COUNT 个文件)"
else
  echo "  缺失: prompts/ 目录为空" >&2
  ALL_OK=false
fi

# 验证 Skills 目录（四个统一 skill：gc:research/plan/execute/review）
for skill in research plan execute review; do
  if [[ -f "$OUTPUT_DIR/skills/$skill/SKILL.md" ]]; then
    echo "  OK: skills/$skill/SKILL.md"
  else
    echo "  缺失: skills/$skill/SKILL.md" >&2
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
