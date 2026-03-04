#!/usr/bin/env bash
# @file release-assets-contract.sh
# @description Release 产物契约测试脚本
#              验证双二进制跨平台构建产物的完整性：
#              1. 六个原始二进制（ghostcoded-* 和 ghostcode-mcp-*，三平台各两个）
#              2. 三个平台 bundle（每个 tar.gz 内包含两个二进制）
#              3. SHA256SUMS 覆盖所有 bundle
# @author Atlas.oi
# @date 2026-03-04

set -euo pipefail

# ============================================================
# 参数解析
# ASSET_DIR：包含所有构建产物的目录
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

# 测试计数器
# 注意：在 set -e 模式下，((VAR++)) 当 VAR 为 0 时退出码为 1 会触发 shell 退出
# 使用 VAR=$((VAR + 1)) 形式避免此问题
PASS=0
FAIL=0

# 辅助函数：断言文件存在
assert_file_exists() {
  local filepath="$1"
  local desc="$2"
  if [[ -f "${filepath}" ]]; then
    echo "[通过] ${desc}"
    PASS=$((PASS + 1))
  else
    echo "[失败] ${desc}"
    echo "       期望文件存在：${filepath}"
    FAIL=$((FAIL + 1))
  fi
}

# 辅助函数：断言 tar.gz 内包含指定文件
assert_tar_contains() {
  local tarfile="$1"
  local member="$2"
  local desc="$3"
  if [[ ! -f "${tarfile}" ]]; then
    echo "[失败] ${desc}（tar 文件不存在：${tarfile}）"
    FAIL=$((FAIL + 1))
    return
  fi
  if tar -tzf "${tarfile}" 2>/dev/null | grep -q "^${member}$"; then
    echo "[通过] ${desc}"
    PASS=$((PASS + 1))
  else
    echo "[失败] ${desc}"
    echo "       ${tarfile} 中未找到：${member}"
    echo "       实际内容：$(tar -tzf "${tarfile}" 2>/dev/null | head -20)"
    FAIL=$((FAIL + 1))
  fi
}

# 辅助函数：断言文件内容包含指定字符串
assert_file_contains() {
  local filepath="$1"
  local pattern="$2"
  local desc="$3"
  if [[ ! -f "${filepath}" ]]; then
    echo "[失败] ${desc}（文件不存在：${filepath}）"
    FAIL=$((FAIL + 1))
    return
  fi
  if grep -q "${pattern}" "${filepath}" 2>/dev/null; then
    echo "[通过] ${desc}"
    PASS=$((PASS + 1))
  else
    echo "[失败] ${desc}"
    echo "       ${filepath} 中未找到：${pattern}"
    FAIL=$((FAIL + 1))
  fi
}

echo "============================================================"
echo "Release 产物契约测试"
echo "资产目录：${ASSET_DIR}"
echo "============================================================"

# ============================================================
# 测试组 1：验证六个原始二进制存在
# ghostcoded-{platform} 和 ghostcode-mcp-{platform}
# 平台：aarch64-apple-darwin, x86_64-apple-darwin, x86_64-unknown-linux-gnu
# ============================================================
echo ""
echo "--- 测试组 1：原始二进制文件 ---"

assert_file_exists "${ASSET_DIR}/ghostcoded-aarch64-apple-darwin"    "ghostcoded macOS ARM64 原始二进制"
assert_file_exists "${ASSET_DIR}/ghostcoded-x86_64-apple-darwin"     "ghostcoded macOS x64 原始二进制"
assert_file_exists "${ASSET_DIR}/ghostcoded-x86_64-unknown-linux-gnu" "ghostcoded Linux x64 原始二进制"
assert_file_exists "${ASSET_DIR}/ghostcode-mcp-aarch64-apple-darwin"    "ghostcode-mcp macOS ARM64 原始二进制"
assert_file_exists "${ASSET_DIR}/ghostcode-mcp-x86_64-apple-darwin"     "ghostcode-mcp macOS x64 原始二进制"
assert_file_exists "${ASSET_DIR}/ghostcode-mcp-x86_64-unknown-linux-gnu" "ghostcode-mcp Linux x64 原始二进制"

# ============================================================
# 测试组 2：验证三个平台 bundle（tar.gz）存在
# ============================================================
echo ""
echo "--- 测试组 2：平台 Bundle tar.gz ---"

assert_file_exists "${ASSET_DIR}/ghostcode-darwin-arm64.tar.gz" "macOS ARM64 平台 bundle"
assert_file_exists "${ASSET_DIR}/ghostcode-darwin-x64.tar.gz"   "macOS x64 平台 bundle"
assert_file_exists "${ASSET_DIR}/ghostcode-linux-x64.tar.gz"    "Linux x64 平台 bundle"

# ============================================================
# 测试组 3：验证每个 bundle 内同时包含两个二进制
# ============================================================
echo ""
echo "--- 测试组 3：Bundle 内容验证（每个 bundle 含双二进制）---"

assert_tar_contains "${ASSET_DIR}/ghostcode-darwin-arm64.tar.gz" "ghostcoded"    "darwin-arm64 bundle 含 ghostcoded"
assert_tar_contains "${ASSET_DIR}/ghostcode-darwin-arm64.tar.gz" "ghostcode-mcp" "darwin-arm64 bundle 含 ghostcode-mcp"

assert_tar_contains "${ASSET_DIR}/ghostcode-darwin-x64.tar.gz" "ghostcoded"    "darwin-x64 bundle 含 ghostcoded"
assert_tar_contains "${ASSET_DIR}/ghostcode-darwin-x64.tar.gz" "ghostcode-mcp" "darwin-x64 bundle 含 ghostcode-mcp"

assert_tar_contains "${ASSET_DIR}/ghostcode-linux-x64.tar.gz" "ghostcoded"    "linux-x64 bundle 含 ghostcoded"
assert_tar_contains "${ASSET_DIR}/ghostcode-linux-x64.tar.gz" "ghostcode-mcp" "linux-x64 bundle 含 ghostcode-mcp"

# ============================================================
# 测试组 4：验证 SHA256SUMS 覆盖所有 bundle
# ============================================================
echo ""
echo "--- 测试组 4：SHA256SUMS 校验文件 ---"

assert_file_exists "${ASSET_DIR}/ghostcode_SHA256SUMS" "ghostcode_SHA256SUMS 文件存在"
assert_file_contains "${ASSET_DIR}/ghostcode_SHA256SUMS" "ghostcode-darwin-arm64.tar.gz" "SHA256SUMS 包含 darwin-arm64 bundle 校验和"
assert_file_contains "${ASSET_DIR}/ghostcode_SHA256SUMS" "ghostcode-darwin-x64.tar.gz"   "SHA256SUMS 包含 darwin-x64 bundle 校验和"
assert_file_contains "${ASSET_DIR}/ghostcode_SHA256SUMS" "ghostcode-linux-x64.tar.gz"    "SHA256SUMS 包含 linux-x64 bundle 校验和"

# ============================================================
# 汇总测试结果
# ============================================================
echo ""
echo "============================================================"
echo "测试结果汇总"
echo "  通过：${PASS}"
echo "  失败：${FAIL}"
echo "============================================================"

if [[ "${FAIL}" -gt 0 ]]; then
  echo "[契约测试失败] 共 ${FAIL} 项未通过"
  exit 1
else
  echo "[契约测试通过] 所有 ${PASS} 项验证成功"
  exit 0
fi
