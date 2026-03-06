/**
 * @file scripts/hook-user-prompt-submit.mjs
 * @description UserPromptSubmit Hook 脚本
 *              在用户提交 prompt 时检测 Magic Keywords 并注入上下文。
 *
 *              输入：从 stdin 读取事件 JSON
 *              输出：到 stdout 输出响应 JSON（含 additionalContext）
 *
 *              参考: src/plugin/src/hooks/handlers.ts - userPromptSubmitHandler
 * @author Atlas.oi
 * @date 2026-03-05
 */

import { join, dirname } from "node:path";
import { readStdin } from "./lib/stdin.mjs";

// ============================================
// 常量
// ============================================

// Plugin 根目录
const PLUGIN_ROOT = process.env.CLAUDE_PLUGIN_ROOT || join(dirname(new URL(import.meta.url).pathname), "..");

// 工作区根目录（Claude Code 保证 process.cwd() 等于项目根目录）
const WORKSPACE_ROOT = process.cwd();

// Magic Keyword 激活后注入给 Claude 的上下文说明
const KEYWORD_CONTEXT_MAP = {
  ralph: "[GhostCode] Ralph 验证模式已激活 - 代码变更将经过 7 项自动验证",
  autopilot: "[GhostCode] Autopilot 模式已激活 - 全自动执行模式",
  team: "[GhostCode] Team 模式已激活 - 多 Agent 协作模式",
  ultrawork: "[GhostCode] UltraWork 模式已激活 - 极致工作模式",
};

// ============================================
// 主逻辑
// ============================================

async function main() {
  // ============================================
  // 第一步：从 stdin 读取事件并提取 prompt
  // ============================================
  let prompt = "";
  try {
    const raw = await readStdin();
    if (raw) {
      const event = JSON.parse(raw);
      // 防御性提取：支持多种事件格式
      prompt = event?.event?.prompt || event?.prompt || "";
    }
  } catch {
    // 解析失败，无 prompt 可处理
  }

  if (!prompt) {
    // 无 prompt，输出空 JSON 不干扰
    console.log(JSON.stringify({}));
    return;
  }

  // ============================================
  // 第二步：追加到 Skill Learning 缓冲区
  // ============================================
  try {
    const { appendSessionContent } = await import(join(PLUGIN_ROOT, "dist", "learner", "manager.js"));
    appendSessionContent(prompt);
  } catch {
    // Skill Learning 模块加载失败不影响主流程
  }

  // ============================================
  // 第三步：检测 Magic Keywords
  // ============================================
  let topMatch = null;
  try {
    const { detectMagicKeywords, resolveKeywordPriority } = await import(join(PLUGIN_ROOT, "dist", "keywords", "index.js"));
    const matches = detectMagicKeywords(prompt);
    topMatch = resolveKeywordPriority(matches);
  } catch (err) {
    console.error("[GhostCode] Keywords 模块加载失败:", err);
  }

  if (topMatch === null) {
    // 无关键词命中，输出空 JSON
    console.log(JSON.stringify({}));
    return;
  }

  // ============================================
  // 第四步：处理 cancel 特殊逻辑
  // ============================================
  if (topMatch.type === "cancel") {
    try {
      const { writeKeywordState } = await import(join(PLUGIN_ROOT, "dist", "keywords", "state.js"));
      await writeKeywordState(WORKSPACE_ROOT, {
        active: null,
        activatedAt: null,
        prompt: null,
      });
    } catch {
      // 状态写入失败不阻断流程
    }
    console.log(JSON.stringify({ additionalContext: "[GhostCode] 模式已取消" }));
    return;
  }

  // ============================================
  // 第五步：激活关键词模式
  // ============================================
  try {
    const { writeKeywordState } = await import(join(PLUGIN_ROOT, "dist", "keywords", "state.js"));
    await writeKeywordState(WORKSPACE_ROOT, {
      active: topMatch.type,
      activatedAt: new Date().toISOString(),
      prompt,
    });
  } catch {
    // 状态写入失败不阻断流程
  }

  // 构建上下文注入信息
  const contextMessage = KEYWORD_CONTEXT_MAP[topMatch.type] ?? `[GhostCode] ${topMatch.type} 模式已激活`;

  // 输出到 stdout
  console.log(JSON.stringify({ additionalContext: contextMessage }));
}

// 执行主逻辑
main().catch((err) => {
  console.error("[GhostCode] hook-user-prompt-submit 异常:", err);
  // 即使异常也输出空 JSON，不阻断流程
  console.log(JSON.stringify({}));
  process.exit(0);
});
