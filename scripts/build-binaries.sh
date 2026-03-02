#!/usr/bin/env bash
# scripts/build-binaries.sh — GhostCode 本地跨平台构建脚本
#
# 在本地开发机上构建三平台二进制。
# macOS 上构建两个 macOS target；Linux 二进制需要 Linux 环境。
#
# 用法:
#   bash scripts/build-binaries.sh [--all | --darwin | --linux]
#   --all     构建所有可构建的 target（默认）
#   --darwin  仅构建 macOS 双架构
#   --linux   仅构建 Linux x64（需要 Linux 环境）
#
# 产物输出到 target/<triple>/release/ghostcode-daemon
# 并复制到 binaries/ 目录（命名含平台后缀）
#
# @author Atlas.oi
# @date 2026-03-01

set -euo pipefail

# 脚本所在目录的父目录（项目根）
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINARIES_DIR="$REPO_ROOT/binaries"

# ============================================
# 参数解析
# ============================================
BUILD_DARWIN=true
BUILD_LINUX=true

case "${1:-}" in
  --darwin) BUILD_LINUX=false ;;
  --linux)  BUILD_DARWIN=false ;;
  --all|"") ;;
  *) echo "未知参数: $1" >&2; echo "用法: $0 [--all | --darwin | --linux]" >&2; exit 1 ;;
esac

# ============================================
# 检测当前平台
# ============================================
CURRENT_OS="$(uname -s)"
CURRENT_ARCH="$(uname -m)"
echo "当前平台: $CURRENT_OS / $CURRENT_ARCH"

# ============================================
# 准备输出目录
# ============================================
mkdir -p "$BINARIES_DIR"

# ============================================
# 函数：构建指定 target 并复制到 binaries/
# ============================================
build_target() {
  local target="$1"
  local output_name="$2"

  echo ""
  echo "构建 target: $target -> $output_name"

  # 确保 target 已添加到 rustup
  rustup target add "$target" 2>/dev/null || true

  # release 模式构建，仅编译 ghostcode-daemon 包
  cargo build --release --target "$target" -p ghostcode-daemon

  # 复制并重命名
  local src="$REPO_ROOT/target/$target/release/ghostcode-daemon"
  local dst="$BINARIES_DIR/$output_name"

  if [[ ! -f "$src" ]]; then
    echo "错误: 构建产物不存在: $src" >&2
    exit 1
  fi

  cp "$src" "$dst"
  chmod +x "$dst"
  echo "  产物: $dst"
}

# ============================================
# 构建 macOS 双架构
# ============================================
if [[ "$BUILD_DARWIN" == "true" ]]; then
  if [[ "$CURRENT_OS" != "Darwin" ]]; then
    echo "警告: 不在 macOS 环境，跳过 macOS 构建" >&2
  else
    build_target "aarch64-apple-darwin" "ghostcoded-darwin-arm64"
    build_target "x86_64-apple-darwin" "ghostcoded-darwin-x64"
  fi
fi

# ============================================
# 构建 Linux x64
# ============================================
if [[ "$BUILD_LINUX" == "true" ]]; then
  if [[ "$CURRENT_OS" != "Linux" ]]; then
    echo "警告: 不在 Linux 环境，跳过 Linux 构建"
    echo "  提示: 可在 Linux 环境或 Docker 中运行:"
    echo "    docker run --rm -v \$(pwd):/workspace rust:1.75 \\"
    echo "      bash /workspace/scripts/build-binaries.sh --linux"
  else
    build_target "x86_64-unknown-linux-gnu" "ghostcoded-linux-x64"
  fi
fi

# ============================================
# 显示构建结果
# ============================================
echo ""
echo "构建结果："
ls -lh "$BINARIES_DIR/" 2>/dev/null || echo "  （无产物）"
