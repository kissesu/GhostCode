/**
 * @file manager.ts
 * @description Skill Learning 管理器
 *              协调检测-提取-写入流水线，集成 Rust Daemon
 *              在会话结束时（Stop Hook）触发学习流程
 * @author Atlas.oi
 * @date 2026-03-03
 */

import { callDaemon } from "../ipc.js";
import { detectPatterns } from "./detector.js";
import type { PatternDetection } from "./types.js";

// 累积的会话内容（Stop 时触发分析）
let sessionContent = "";

/**
 * 追加会话内容（由 UserPromptSubmit Hook 调用）
 *
 * @param content - 本次 prompt 内容
 */
export function appendSessionContent(content: string): void {
  sessionContent += `\n${content}`;
}

/**
 * 会话结束时触发 Skill Learning 分析
 *
 * 业务逻辑：
 * 1. 检测会话内容中的模式
 * 2. 对置信度 >= 70 的候选调用 daemon skill_learn_fragment
 * 3. 重置会话内容
 *
 * 错误不阻断 Stop Hook 主流程（静默处理）
 */
export async function onSessionEnd(): Promise<void> {
  const content = sessionContent;
  sessionContent = "";

  if (content.length < 100) return;

  const patterns = detectPatterns(content);

  for (const pattern of patterns) {
    if (pattern.confidence < 70) continue;

    try {
      await callDaemon("skill_learn_fragment", {
        problem: pattern.problem,
        solution: pattern.solution,
        confidence: pattern.confidence,
        context: content.slice(0, 500),
        suggested_triggers: pattern.suggestedTriggers,
        suggested_tags: pattern.suggestedTags,
      });
    } catch {
      // 静默处理：Skill Learning 失败不影响主流程
    }
  }
}

/**
 * 列出 Daemon 中的所有 Skill 候选
 */
export async function listCandidates(): Promise<PatternDetection[]> {
  try {
    const resp = await callDaemon("skill_list");
    if (resp.ok && Array.isArray(resp.result)) {
      return resp.result as PatternDetection[];
    }
  } catch {
    // 静默处理
  }
  return [];
}
