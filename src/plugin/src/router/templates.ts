/**
 * @file 命令模板引擎
 * @description 提供 {{VARIABLE}} 变量替换、代码主权规则注入等模板处理能力。
 *              非 Claude 后端的任务提示词自动追加代码主权规则，确保外部模型零写入权限。
 * @author Atlas.oi
 * @date 2026-03-02
 */

import type { BackendName } from "./types";

/**
 * 代码主权规则常量
 *
 * 业务逻辑：
 * - 外部模型（非 Claude）只能以文本形式输出建议，不得直接操作文件系统
 * - Claude 作为审核方，负责将建议转换为实际的代码修改
 */
export const SOVEREIGNTY_RULE =
  "严禁对文件系统进行任何写入操作。所有代码修改建议必须以文本形式输出，由 Claude 审核后执行。";

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
export function renderTemplate(
  template: string,
  vars: Record<string, string>
): string {
  // 匹配 {{KEY}} 形式的占位符，KEY 允许字母、数字、下划线
  return template.replace(/\{\{([A-Z0-9_]+)\}\}/g, (match, key: string): string => {
    // 未在 vars 中找到对应键时，保留原始占位符
    const value = vars[key];
    return value !== undefined ? value : match;
  });
}

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
export function buildTaskPrompt(
  task: string,
  backend: BackendName,
  vars: Record<string, string>
): string {
  // 第一步：执行变量替换
  const rendered = renderTemplate(task, vars);

  // 第二步：非 Claude 后端追加代码主权规则
  // Claude 是唯一拥有文件写入权限的后端，其他后端必须受到限制
  if (backend !== "claude") {
    return `${rendered}\n\n${SOVEREIGNTY_RULE}`;
  }

  return rendered;
}
