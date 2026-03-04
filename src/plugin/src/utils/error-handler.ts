/**
 * @file 统一错误处理模块
 * @description 将内部错误（NodeJS 系统错误、业务逻辑错误等）转化为用户可读的结构化
 *              错误信息，每个错误均包含唯一错误码（GC_XXX_NNN）和修复建议。
 *
 *              核心函数：
 *              - formatErrorWithFix：将原始 Error 转换为 UserFacingError
 *              - formatErrorAsMarkdown：将 UserFacingError 格式化为 Markdown 字符串
 *
 *              匹配逻辑（优先级从高到低）：
 *              1. NodeJS errno code（如 ECONNREFUSED）
 *              2. 错误消息中包含已知 key（子串匹配）
 *              3. 回退到 UNKNOWN_ERROR_TEMPLATE
 * @author Atlas.oi
 * @date 2026-03-04
 */

import {
  ERROR_TEMPLATES,
  UNKNOWN_ERROR_TEMPLATE,
  type ErrorTemplate,
} from "../templates/error-messages";

// ============================================
// 对外暴露的类型定义
// ============================================

/** 用户可见的结构化错误，包含错误码和修复建议 */
export interface UserFacingError {
  /** 错误码，格式 GC_{CATEGORY}_{NNN} */
  code: string;
  /** 简短标题 */
  title: string;
  /** 详细描述 */
  description: string;
  /** 修复建议（面向用户的自然语言） */
  suggestion: string;
  /** 可执行的修复命令（可选） */
  fixCommand?: string;
  /** 保留原始错误对象以供调试 */
  originalError?: Error;
}

// ============================================
// 核心实现
// ============================================

/**
 * 根据原始错误查找最匹配的错误模板
 *
 * 匹配优先级：
 * 1. NodeJS errno code 精确匹配（如 ECONNREFUSED）
 * 2. 错误消息字符串中包含已知 key（子串匹配，不区分大小写）
 * 3. 返回 null 触发 UNKNOWN 回退
 *
 * @param error - 原始错误对象或错误消息字符串
 * @returns 匹配到的模板，未匹配返回 null
 */
function matchErrorTemplate(error: Error | string): ErrorTemplate | null {
  // ----------------------------------------
  // 步骤 1：从 NodeJS errno code 精确匹配
  // NodeJS 系统错误（如 net.connect 失败）会携带 .code 属性
  // 使用 Object.prototype.hasOwnProperty 避免 noUncheckedIndexedAccess 误判
  // ----------------------------------------
  if (typeof error !== "string") {
    const errnoCode = (error as NodeJS.ErrnoException).code;
    if (errnoCode && Object.prototype.hasOwnProperty.call(ERROR_TEMPLATES, errnoCode)) {
      // 经过 hasOwnProperty 检查后断言为非 undefined
      return ERROR_TEMPLATES[errnoCode] as ErrorTemplate;
    }
  }

  // ----------------------------------------
  // 步骤 2：从错误消息中进行子串匹配
  // 适用于业务逻辑抛出的带有标识符的错误，如 new Error("BINARY_NOT_FOUND")
  // ----------------------------------------
  const message = typeof error === "string" ? error : error.message;
  const messageUpper = message.toUpperCase();

  for (const key of Object.keys(ERROR_TEMPLATES)) {
    if (messageUpper.includes(key.toUpperCase())) {
      // 经过 Object.keys 枚举的 key 必然存在于对象中
      return ERROR_TEMPLATES[key] as ErrorTemplate;
    }
  }

  // 未匹配任何模板
  return null;
}

/**
 * 将原始错误转化为用户可读的结构化错误信息
 *
 * 业务逻辑：
 * 1. 尝试从 errno code 或消息中查找预定义模板
 * 2. 命中则使用预定义模板填充 UserFacingError
 * 3. 未命中则使用 UNKNOWN_ERROR_TEMPLATE 作为回退，code 为 GC_UNKNOWN_000
 * 4. 始终保留 originalError 字段以供调试
 *
 * @param error - 原始 Error 对象或错误消息字符串
 * @returns 结构化的用户可见错误
 */
export function formatErrorWithFix(error: Error | string): UserFacingError {
  // 将字符串统一包装为 Error 对象，便于统一处理
  const errorObj = typeof error === "string" ? new Error(error) : error;

  // 查找匹配的模板
  const template = matchErrorTemplate(errorObj) ?? UNKNOWN_ERROR_TEMPLATE;

  // ----------------------------------------
  // 使用条件展开避免 exactOptionalPropertyTypes 的 undefined 赋值问题：
  // 直接写 fixCommand: template.fixCommand 时，若值为 undefined，TS 会报错。
  // 条件展开（...spread）只在值存在时插入该字段。
  // ----------------------------------------
  return {
    code: template.code,
    title: template.title,
    description: template.description,
    suggestion: template.suggestion,
    ...(template.fixCommand !== undefined && { fixCommand: template.fixCommand }),
    // 保留原始错误对象，方便上层记录日志或调试
    originalError: errorObj,
  };
}

/**
 * 将 UserFacingError 格式化为 Markdown 字符串
 *
 * 输出格式示例：
 * ```
 * **[GC_IPC_001] Daemon 连接失败**
 *
 * 无法连接到 GhostCode Daemon，可能是 Daemon 未启动或 socket 文件不存在
 *
 * **修复建议：** 请运行 `ghostcode doctor` 检查 Daemon 状态
 *
 * **修复命令：**
 * ```sh
 * ghostcode doctor
 * ```
 * ```
 *
 * @param error - 结构化的用户可见错误
 * @returns Markdown 格式的错误报告字符串
 */
export function formatErrorAsMarkdown(error: UserFacingError): string {
  const lines: string[] = [];

  // ----------------------------------------
  // 标题行：粗体 + 错误码 + 标题
  // ----------------------------------------
  lines.push(`**[${error.code}] ${error.title}**`);
  lines.push("");

  // ----------------------------------------
  // 描述段落
  // ----------------------------------------
  lines.push(error.description);
  lines.push("");

  // ----------------------------------------
  // 修复建议（内联代码格式化命令）
  // ----------------------------------------
  lines.push(`**修复建议：** ${error.suggestion}`);

  // ----------------------------------------
  // 如果有可执行命令，单独用代码块展示
  // ----------------------------------------
  if (error.fixCommand) {
    lines.push("");
    lines.push("**修复命令：**");
    lines.push("```sh");
    lines.push(error.fixCommand);
    lines.push("```");
  }

  return lines.join("\n");
}
