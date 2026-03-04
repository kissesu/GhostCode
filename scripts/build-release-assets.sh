#!/usr/bin/env bash
# @file build-release-assets.sh
# @description Release 产物组装脚本
#              接收包含六个原始二进制的 ASSET_DIR，将同平台的两个二进制
#              (ghostcoded + ghostcode-mcp) 打包为平台 bundle tar.gz
#
#              平台映射：
#                aarch64-apple-darwin    -> darwin-arm64
#                x86_64-apple-darwin     -> darwin-x64
#                x86_64-unknown-linux-gnu -> linux-x64
#
#              产出文件：
#                ghostcode-darwin-arm64.tar.gz
#                ghostcode-darwin-x64.tar.gz
#                ghostcode-linux-x64.tar.gz
#
# @author Atlas.oi
# @date 2026-03-04

set -euo pipefail

# ============================================================
# 参数解析
# ASSET_DIR：包含六个原始二进制的目录（由 CI 编译步骤产出）
# ============================================================
ASSET_DIR="${1:-}"
if [[ -z "${ASSET_DIR}" ]]; then
  echo "[错误] 用法：$0 <ASSET_DIR>"
  exit 1
fi

if [[ ! -d "${ASSET_DIR}" ]]; then
  echo "[错误] 目录不存在：${ASSET_DIR}"
  exit 1
fi

# ============================================================
# 前置检查：缺失任何二进制时立即失败
# 双二进制完整性是核心保障，不允许部分成功
# ============================================================
echo "[检查] 验证六个原始二进制完整性..."

# 检查 ghostcoded 三平台二进制
for target in aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-gnu; do
  binary="${ASSET_DIR}/ghostcoded-${target}"
  if [[ ! -f "${binary}" ]]; then
    echo "[错误] 缺失 ghostcoded 二进制：${binary}"
    exit 1
  fi
done

# 检查 ghostcode-mcp 三平台二进制（缺失时必须失败，禁止降级）
for target in aarch64-apple-darwin x86_64-apple-darwin x86_64-unknown-linux-gnu; do
  binary="${ASSET_DIR}/ghostcode-mcp-${target}"
  if [[ ! -f "${binary}" ]]; then
    echo "[错误] 缺失 ghostcode-mcp 二进制：${binary}"
    echo "[错误] ghostcode-mcp 是核心组件，不允许跳过，构建终止"
    exit 1
  fi
done

echo "[通过] 六个原始二进制均存在"

# ============================================================
# assemble_platform_bundle 函数
# 将同一平台的 ghostcoded 和 ghostcode-mcp 打包为 tar.gz
#
# 参数：
#   $1 - target：Rust 编译目标三元组（如 aarch64-apple-darwin）
#   $2 - platform_name：平台短名称（如 darwin-arm64）
# ============================================================
assemble_platform_bundle() {
  local target="$1"
  local platform_name="$2"
  local bundle_name="ghostcode-${platform_name}.tar.gz"

  echo "[组装] 正在组装 ${bundle_name}..."

  # 创建临时工作目录，组装完毕后清理
  local tmpdir
  tmpdir=$(mktemp -d)
  trap "rm -rf ${tmpdir}" RETURN

  # ============================================================
  # 复制两个二进制到临时目录
  # 去掉平台后缀，tar 内文件名为 ghostcoded 和 ghostcode-mcp
  # ============================================================
  cp "${ASSET_DIR}/ghostcoded-${target}"    "${tmpdir}/ghostcoded"
  cp "${ASSET_DIR}/ghostcode-mcp-${target}" "${tmpdir}/ghostcode-mcp"

  # 赋予执行权限
  chmod +x "${tmpdir}/ghostcoded" "${tmpdir}/ghostcode-mcp"

  # ============================================================
  # 打包为 tar.gz，输出到 ASSET_DIR
  # -C 切换到临时目录，避免路径前缀污染
  # ============================================================
  tar -czf "${ASSET_DIR}/${bundle_name}" \
    -C "${tmpdir}" \
    ghostcoded \
    ghostcode-mcp

  echo "[完成] ${bundle_name} 已生成"
}

# ============================================================
# 主流程：组装三个平台 bundle
# ============================================================
echo ""
echo "[开始] 组装平台 Bundle..."

assemble_platform_bundle "aarch64-apple-darwin"    "darwin-arm64"
assemble_platform_bundle "x86_64-apple-darwin"     "darwin-x64"
assemble_platform_bundle "x86_64-unknown-linux-gnu" "linux-x64"

echo ""
echo "[完成] 所有平台 bundle 组装完成"
echo "       输出目录：${ASSET_DIR}"
ls -lh "${ASSET_DIR}"/ghostcode-*.tar.gz 2>/dev/null || true
