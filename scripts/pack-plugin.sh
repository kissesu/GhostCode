#!/usr/bin/env bash
# scripts/pack-plugin.sh — GhostCode Plugin 打包脚本
#
# 将三平台 Rust 二进制 + TypeScript 编译产物 + 配置文件
# 组装为符合 Claude Code Plugin 规范的 npm 包目录。
#
# 用法:
#   bash scripts/pack-plugin.sh \
#     --binaries-dir <path>   二进制文件所在目录
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
#       ghostcoded-darwin-arm64
#       ghostcoded-darwin-x64
#       ghostcoded-linux-x64
#     .claude/
#       settings.json
#
# @author Atlas.oi
# @date 2026-03-02

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

if [[ ! -f "$TS_DIST/index.js" ]]; then
  echo "错误: 缺少 TypeScript 产物: $TS_DIST/index.js" >&2
  exit 1
fi

cp "$TS_DIST/index.js" "$OUTPUT_DIR/dist/index.js"
echo "  复制: dist/index.js"

# ============================================
# 第三步：复制三平台二进制
# 每个二进制复制后设置可执行权限
# ============================================
echo "第三步：复制三平台二进制..."

BINARIES=(
  "ghostcoded-darwin-arm64"
  "ghostcoded-darwin-x64"
  "ghostcoded-linux-x64"
)

for bin in "${BINARIES[@]}"; do
  src="$BINARIES_DIR/$bin"
  dst="$OUTPUT_DIR/bin/$bin"

  if [[ ! -f "$src" ]]; then
    echo "错误: 缺少二进制文件: $src" >&2
    exit 1
  fi

  cp "$src" "$dst"
  chmod +x "$dst"
  echo "  复制: bin/$bin"
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
    "url": "https://github.com/ghostcode/ghostcode.git"
  }
}
EOF
echo "  生成: package.json"

# ============================================
# 第五步：生成 .claude/settings.json
# Claude Code Plugin 配置文件
# 声明 Plugin 元信息和 MCP server 配置
# ============================================
echo "第五步：生成 .claude/settings.json..."

cat > "$OUTPUT_DIR/.claude/settings.json" << EOF
{
  "mcpServers": {
    "ghostcode": {
      "type": "stdio",
      "command": "node",
      "args": ["dist/index.js"],
      "description": "GhostCode 多 Agent 协作开发平台"
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
