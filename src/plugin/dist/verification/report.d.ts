import { RunState, ReportFormat, Verdict } from './types.js';

/**
 * @file report.ts
 * @description 验证报告格式化输出
 *              将 RunState 转换为人类可读的 Markdown 或 JSON 报告
 *              参考: oh-my-claudecode/src/skills/ralph.ts - Ralph 验证循环输出
 * @author Atlas.oi
 * @date 2026-03-03
 */

/**
 * 将 Rust RunStatus 字符串映射为语义化 Verdict
 *
 * 业务逻辑说明：
 * - Approved → approved（验证通过）
 * - Rejected → rejected（验证失败，超过最大迭代次数）
 * - Cancelled → cancelled（用户主动取消）
 * - Running 及其他 → in_progress（仍在运行中）
 *
 * @param status - RunStatus 字符串值
 * @returns 语义化的 Verdict 判定结果
 */
declare function mapStatusToVerdict(status: string): Verdict;
/**
 * 格式化验证报告
 *
 * 业务逻辑说明：
 * 1. 根据 format 参数选择 markdown 或 json 输出格式
 * 2. Markdown 格式：输出人类可读的表格式报告，含历史记录
 * 3. JSON 格式：输出结构化数据，适合程序消费
 * 4. 默认为 markdown 格式（适合直接展示给用户）
 *
 * @param state - 当前 RunState 快照
 * @param format - 输出格式，默认为 "markdown"
 * @returns 格式化后的报告字符串
 */
declare function formatReport(state: RunState, format?: ReportFormat): string;

export { formatReport, mapStatusToVerdict };
