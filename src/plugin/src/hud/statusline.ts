#!/usr/bin/env node
/**
 * @file HUD 状态栏 CLI 入口
 * @description ghostcode-hud 命令行工具的入口文件
 *              支持两种工作模式：
 *              1. 从 stdin 读取 JSON（Claude Code statusline 协议）
 *              2. 直接调用 Daemon 的 hud_snapshot op 获取数据
 *              输出渲染后的状态栏字符串到 stdout
 * @author Atlas.oi
 * @date 2026-03-03
 */
import { renderStatusline } from "./render";
import { fetchHudSnapshot } from "./snapshot";
import type { HudSnapshot } from "./types";

/**
 * 从 stdin 读取完整输入（非交互模式）
 *
 * @returns stdin 中的完整字符串内容
 */
async function readStdin(): Promise<string> {
  return new Promise((resolve) => {
    let data = "";
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (chunk: string) => {
      data += chunk;
    });
    process.stdin.on("end", () => {
      resolve(data.trim());
    });
    // stdin 无数据时（非管道输入），立即 resolve 空字符串
    if (process.stdin.isTTY) {
      resolve("");
    }
  });
}

/**
 * 尝试将字符串解析为 HudSnapshot JSON
 *
 * @param input - 待解析的 JSON 字符串
 * @returns 解析成功的 HudSnapshot，或 null（解析失败时）
 */
function tryParseSnapshot(input: string): HudSnapshot | null {
  if (!input) return null;

  try {
    const parsed = JSON.parse(input) as unknown;
    // 简单校验：检查必需字段是否存在
    if (
      typeof parsed === "object" &&
      parsed !== null &&
      "cost" in parsed &&
      "context_pressure" in parsed &&
      "active_agents" in parsed
    ) {
      return parsed as HudSnapshot;
    }
  } catch {
    // JSON 解析失败，忽略错误，回退到 Daemon 调用
  }

  return null;
}

/**
 * CLI 主函数
 *
 * 业务逻辑：
 * 1. 先尝试从 stdin 读取 HudSnapshot JSON（Claude Code statusline 协议）
 * 2. 如果 stdin 为空或解析失败，则调用 Daemon 的 hud_snapshot op
 * 3. 渲染状态栏并输出到 stdout
 * 4. 出错时输出错误信息到 stderr，退出码 1
 */
async function main(): Promise<void> {
  try {
    let snapshot: HudSnapshot;

    // ============================================
    // 第一步：尝试从 stdin 读取 HudSnapshot JSON
    // Claude Code statusline 协议：通过 stdin 传入快照数据
    // ============================================
    const stdinData = await readStdin();
    const parsedSnapshot = tryParseSnapshot(stdinData);

    if (parsedSnapshot !== null) {
      // 从 stdin 成功解析到 HudSnapshot
      snapshot = parsedSnapshot;
    } else {
      // ============================================
      // 第二步：stdin 无数据或解析失败，调用 Daemon
      // 通过 Unix socket IPC 获取实时 HUD 数据
      // ============================================
      snapshot = await fetchHudSnapshot();
    }

    // ============================================
    // 第三步：渲染状态栏并输出到 stdout
    // ============================================
    const modelName = process.env["GHOSTCODE_MODEL"] ?? "unknown";
    const output = renderStatusline(snapshot, { modelName });

    process.stdout.write(output + "\n");
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    process.stderr.write(`ghostcode-hud 错误: ${message}\n`);
    process.exit(1);
  }
}

// 执行主函数
void main();
