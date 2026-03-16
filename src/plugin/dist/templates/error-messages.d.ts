/**
 * @file 错误消息模板定义
 * @description 集中管理所有错误码、错误描述和修复建议的映射表。
 *              错误码格式：GC_{分类}_{三位数字}
 *              分类：IPC / CONFIG / BINARY / RUNTIME / NETWORK / UNKNOWN
 * @author Atlas.oi
 * @date 2026-03-04
 */
/** 错误分类枚举 */
declare enum ErrorCategory {
    /** IPC 通信错误：Unix socket 连接、读写失败等 */
    IPC = "IPC",
    /** 配置错误：TOML 解析失败、配置文件不存在等 */
    CONFIG = "CONFIG",
    /** 二进制错误：ghostcode-daemon 可执行文件缺失或权限不足 */
    BINARY = "BINARY",
    /** 运行时错误：Daemon 崩溃、会话过期等 */
    RUNTIME = "RUNTIME",
    /** 网络错误：下载失败、checksum 校验不通过等 */
    NETWORK = "NETWORK",
    /** 未知错误：无法归类的错误 */
    UNKNOWN = "UNKNOWN"
}
/** 错误消息模板，每条记录对应一类可预知的故障 */
interface ErrorTemplate {
    /** 错误码，格式 GC_{CATEGORY}_{NNN}，如 GC_IPC_001 */
    code: string;
    /** 所属分类 */
    category: ErrorCategory;
    /** 简短标题，用于 Markdown 加粗展示 */
    title: string;
    /** 详细描述，解释错误原因 */
    description: string;
    /** 修复建议，面向用户的操作指导 */
    suggestion: string;
    /** 可执行的修复命令（可选），如 ghostcode doctor */
    fixCommand?: string;
}
/** 预定义错误模板：key 为 NodeJS errno 或内部错误标识符 */
declare const ERROR_TEMPLATES: Record<string, ErrorTemplate>;
/** 未知错误的回退模板 */
declare const UNKNOWN_ERROR_TEMPLATE: ErrorTemplate;

export { ERROR_TEMPLATES, ErrorCategory, type ErrorTemplate, UNKNOWN_ERROR_TEMPLATE };
