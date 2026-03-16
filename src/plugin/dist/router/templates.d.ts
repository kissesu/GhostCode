import { BackendName } from './types.js';

/**
 * @file 命令模板引擎
 * @description 提供 {{VARIABLE}} 变量替换、代码主权规则注入等模板处理能力。
 *              非 Claude 后端的任务提示词自动追加代码主权规则，确保外部模型零写入权限。
 * @author Atlas.oi
 * @date 2026-03-02
 */

/**
 * 代码主权规则常量
 *
 * 业务逻辑：
 * - 外部模型（非 Claude）只能以文本形式输出建议，不得直接操作文件系统
 * - Claude 作为审核方，负责将建议转换为实际的代码修改
 */
declare const SOVEREIGNTY_RULE = "\u4E25\u7981\u5BF9\u6587\u4EF6\u7CFB\u7EDF\u8FDB\u884C\u4EFB\u4F55\u5199\u5165\u64CD\u4F5C\u3002\u6240\u6709\u4EE3\u7801\u4FEE\u6539\u5EFA\u8BAE\u5FC5\u987B\u4EE5\u6587\u672C\u5F62\u5F0F\u8F93\u51FA\uFF0C\u7531 Claude \u5BA1\u6838\u540E\u6267\u884C\u3002";
/**
 * 渲染模板字符串，将 {{KEY}} 占位符替换为对应的变量值
 *
 * 业务逻辑：
 * 1. 使用正则匹配所有 {{KEY}} 形式的占位符
 * 2. 在 vars 中查找对应键名，有则替换，无则保留原样
 * 3. ROLE_FILE: <path> 行为普通文本，不做特殊处理（不干扰占位符替换逻辑）
 *
 * @param template - 包含 {{KEY}} 占位符的模板字符串
 * @param vars - 变量键值映射表
 * @returns 替换后的字符串，未匹配的占位符保持原样
 */
declare function renderTemplate(template: string, vars: Record<string, string>): string;
/**
 * 构建发送给指定后端的任务提示词
 *
 * 业务逻辑：
 * 1. 对任务文本执行变量替换
 * 2. 非 Claude 后端自动在末尾追加代码主权规则，限制外部模型的写入权限
 * 3. Claude 后端作为代码主权持有者，不追加限制规则
 *
 * @param task - 任务描述文本，可包含 {{KEY}} 占位符
 * @param backend - 目标后端名称
 * @param vars - 变量键值映射表，用于替换 task 中的占位符
 * @returns 最终发送给后端的完整提示词
 */
declare function buildTaskPrompt(task: string, backend: BackendName, vars: Record<string, string>): string;

export { SOVEREIGNTY_RULE, buildTaskPrompt, renderTemplate };
