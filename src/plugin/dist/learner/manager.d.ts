import { PatternDetection } from './types.js';

/**
 * @file manager.ts
 * @description Skill Learning 管理器
 *              协调检测-提取-写入流水线，集成 Rust Daemon
 *              在会话结束时（Stop Hook）触发学习流程
 * @author Atlas.oi
 * @date 2026-03-03
 */

/**
 * 追加会话内容（由 UserPromptSubmit Hook 调用）
 *
 * @param content - 本次 prompt 内容
 */
declare function appendSessionContent(content: string): void;
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
declare function onSessionEnd(): Promise<void>;
/**
 * 列出 Daemon 中的所有 Skill 候选
 */
declare function listCandidates(): Promise<PatternDetection[]>;

export { appendSessionContent, listCandidates, onSessionEnd };
