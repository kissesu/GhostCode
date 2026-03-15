#!/usr/bin/env bash
# @file release-checksums.sh
# @description Release SHA256 校验和生成脚本
#              对 ASSET_DIR 内所有 .tar.gz 文件生成 SHA256 校验和
#              输出到 ghostcode_SHA256SUMS 文件
#
#              兼容 macOS（shasum -a 256）和 Linux（sha256sum）
#
# @author Atlas.oi
# @date 2026-03-04

set -euo pipefail

# ============================================================
# 参数解析
# ASSET_DIR：包含 .tar.gz 文件的目录
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

# SHA256SUMS 输出文件名（cd 到 ASSET_DIR 后使用相对路径）
SUMS_FILENAME="ghostcode_SHA256SUMS"

# ============================================================
# 检测平台并选择合适的 sha256 工具
# macOS 使用 shasum -a 256，Linux 使用 sha256sum
# ============================================================
if command -v sha256sum &>/dev/null; then
  SHA256_CMD="sha256sum"
elif command -v shasum &>/dev/null; then
  SHA256_CMD="shasum -a 256"
else
  echo "[错误] 未找到 sha256sum 或 shasum 工具，无法生成校验和"
  exit 1
fi

echo "[开始] 生成 SHA256 校验和..."
echo "       工具：${SHA256_CMD}"
echo "       目录：${ASSET_DIR}"

# ============================================================
# 查找所有 .tar.gz 文件并生成校验和
# 切换到 ASSET_DIR 执行，使校验和文件中只含文件名（无路径前缀）
# ============================================================
cd "${ASSET_DIR}"

# 清空并重新生成 SHA256SUMS
> "${SUMS_FILENAME}"

# 对每个 .tar.gz 文件生成校验和并追加到文件
bundle_count=0
for tarfile in ghostcode-*.tar.gz; do
  if [[ ! -f "${tarfile}" ]]; then
    continue
  fi

  ${SHA256_CMD} "${tarfile}" >> "${SUMS_FILENAME}"
  echo "[校验] ${tarfile}"
  bundle_count=$((bundle_count + 1))
done

# ============================================================
# 验证至少处理了一个文件，否则说明无产物可校验
# ============================================================
if [[ "${bundle_count}" -eq 0 ]]; then
  echo "[错误] 未找到任何 .tar.gz 文件，无法生成校验和"
  exit 1
fi

echo ""
echo "[完成] 已为 ${bundle_count} 个 bundle 生成 SHA256 校验和"
echo "       输出文件：${SUMS_FILENAME}"
echo ""
echo "校验和内容："
cat "${SUMS_FILENAME}"
