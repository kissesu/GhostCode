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
/** 用户可见的结构化错误，包含错误码和修复建议 */
interface UserFacingError {
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
declare function formatErrorWithFix(error: Error | string): UserFacingError;
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
declare function formatErrorAsMarkdown(error: UserFacingError): string;

export { type UserFacingError, formatErrorAsMarkdown, formatErrorWithFix };
