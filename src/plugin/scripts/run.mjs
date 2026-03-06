/**
 * @file scripts/run.mjs
 * @description GhostCode Hook 统一脚本运行器
 *              接收目标脚本路径参数，统一处理环境变量注入、路径解析、错误处理和超时保护。
 *              由 hooks.json 中的命令调用，格式：
 *              node "${CLAUDE_PLUGIN_ROOT}/scripts/run.mjs" "${CLAUDE_PLUGIN_ROOT}/scripts/hook-xxx.mjs"
 *
 *              参考: oh-my-claudecode/scripts/run.cjs - 跨平台 Hook 运行器
 * @author Atlas.oi
 * @date 2026-03-05
 */

import { spawnSync } from "node:child_process";
import { existsSync, realpathSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { homedir } from "node:os";
import { fileURLToPath } from "node:url";

// 当前脚本所在目录（ESM 中没有 __dirname，需要手动计算）
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// 脚本超时时间（毫秒）：防止 Hook 脚本挂起阻塞 Claude Code
const SCRIPT_TIMEOUT_MS = 30_000;

// ============================================
// 第一步：获取目标脚本路径
// ============================================
const target = process.argv[2];
if (!target) {
  // 无参数时静默退出，不阻断 Claude Code hooks
  process.exit(0);
}

// ============================================
// 第二步：解析目标脚本路径
// 处理 symlink 和路径不存在的边界情况
// ============================================
function resolveTarget(targetPath) {
  // 快速路径：目标直接存在
  if (existsSync(targetPath)) return targetPath;

  // 尝试 symlink 解析
  try {
    const resolved = realpathSync(targetPath);
    if (existsSync(resolved)) return resolved;
  } catch {
    // realpathSync 在路径不存在时抛异常，预期行为
  }

  return null;
}

const resolved = resolveTarget(target);
if (!resolved) {
  // 目标脚本不存在，静默退出不阻断 hooks
  process.exit(0);
}

// ============================================
// 第三步：注入环境变量
// ============================================
const env = { ...process.env };

// CLAUDE_PLUGIN_ROOT：从当前脚本路径推导（scripts/ 的父目录）
if (!env.CLAUDE_PLUGIN_ROOT) {
  env.CLAUDE_PLUGIN_ROOT = resolve(__dirname, "..");
}

// GHOSTCODE_HOME：GhostCode 数据目录，默认 ~/.ghostcode
if (!env.GHOSTCODE_HOME) {
  env.GHOSTCODE_HOME = resolve(homedir(), ".ghostcode");
}

// ============================================
// 第四步：执行目标脚本
// 使用 spawnSync 同步执行，继承 stdio 流
// ============================================
// stdio 使用数组形式显式指定三个流，确保子进程能读取父进程 stdin
// - stdin (fd 0): "inherit" — 父进程 stdin 透传给子进程，使 hook 脚本能通过 readStdin 读取事件 JSON
// - stdout (fd 1): "inherit" — 子进程输出直接写入父进程 stdout，供 Claude Code 读取 Hook 响应
// - stderr (fd 2): "inherit" — 子进程错误直接写入父进程 stderr，便于调试
const result = spawnSync(
  process.execPath,
  [resolved, ...process.argv.slice(3)],
  {
    stdio: ["inherit", "inherit", "inherit"],
    env,
    timeout: SCRIPT_TIMEOUT_MS,
    windowsHide: true,
  }
);

// ============================================
// 第五步：传播退出码
// null（超时或信号终止）时返回 0，避免阻断 hooks
// ============================================
process.exit(result.status ?? 0);
