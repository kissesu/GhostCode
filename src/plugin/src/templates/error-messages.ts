/**
 * @file 错误消息模板定义
 * @description 集中管理所有错误码、错误描述和修复建议的映射表。
 *              错误码格式：GC_{分类}_{三位数字}
 *              分类：IPC / CONFIG / BINARY / RUNTIME / NETWORK / UNKNOWN
 * @author Atlas.oi
 * @date 2026-03-04
 */

// ============================================
// 错误分类枚举
// 对应不同的子系统故障域
// ============================================

/** 错误分类枚举 */
export enum ErrorCategory {
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
  UNKNOWN = "UNKNOWN",
}

// ============================================
// 错误消息模板接口
// ============================================

/** 错误消息模板，每条记录对应一类可预知的故障 */
export interface ErrorTemplate {
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

// ============================================
// 预定义错误模板映射表
// key 为错误码（NodeJS errno code 或内部错误标识符）
// ============================================

/** 预定义错误模板：key 为 NodeJS errno 或内部错误标识符 */
export const ERROR_TEMPLATES: Record<string, ErrorTemplate> = {
  // ----------------------------------------
  // IPC 通信错误（GC_IPC_xxx）
  // 对应 Daemon 与 Plugin 之间的 Unix socket 通信故障
  // ----------------------------------------

  /** connect ECONNREFUSED：Daemon 未启动或 socket 不存在 */
  ECONNREFUSED: {
    code: "GC_IPC_001",
    category: ErrorCategory.IPC,
    title: "Daemon 连接失败",
    description: "无法连接到 GhostCode Daemon，可能是 Daemon 未启动或 socket 文件不存在",
    suggestion: "请运行 `ghostcode doctor` 检查 Daemon 状态",
    fixCommand: "ghostcode doctor",
  },

  /** socket 文件不存在：IPC 通道尚未建立 */
  SOCKET_NOT_FOUND: {
    code: "GC_IPC_002",
    category: ErrorCategory.IPC,
    title: "Socket 文件不存在",
    description: "GhostCode Daemon 的 Unix socket 文件未找到",
    suggestion: "请运行 `ghostcode doctor` 重新启动 Daemon",
    fixCommand: "ghostcode doctor",
  },

  /** socket 超时：Daemon 无响应 */
  SOCKET_TIMEOUT: {
    code: "GC_IPC_003",
    category: ErrorCategory.IPC,
    title: "IPC 连接超时",
    description: "连接 Daemon 超时，Daemon 可能无响应或负载过高",
    suggestion: "请运行 `ghostcode doctor` 检查 Daemon 状态或重启",
    fixCommand: "ghostcode doctor",
  },

  // ----------------------------------------
  // 配置错误（GC_CONFIG_xxx）
  // TOML 配置文件读取或解析失败
  // ----------------------------------------

  /** TOML 格式错误 */
  CONFIG_PARSE_ERROR: {
    code: "GC_CONFIG_001",
    category: ErrorCategory.CONFIG,
    title: "配置文件解析失败",
    description: "GhostCode 配置文件格式错误，无法解析 TOML",
    suggestion: "请检查 ~/.ghostcode/config.toml 的格式是否正确",
  },

  /** 配置文件不存在 */
  CONFIG_NOT_FOUND: {
    code: "GC_CONFIG_002",
    category: ErrorCategory.CONFIG,
    title: "配置文件不存在",
    description: "未找到 GhostCode 配置文件",
    suggestion: "请运行 `ghostcode init` 初始化配置",
    fixCommand: "ghostcode init",
  },

  // ----------------------------------------
  // 二进制错误（GC_BINARY_xxx）
  // ghostcode-daemon 可执行文件相关故障
  // ----------------------------------------

  /** Daemon 可执行文件未找到 */
  BINARY_NOT_FOUND: {
    code: "GC_BINARY_001",
    category: ErrorCategory.BINARY,
    title: "Daemon 二进制文件不存在",
    description: "找不到 ghostcode-daemon 可执行文件，可能未安装或路径配置错误",
    suggestion: "请运行 `ghostcode init` 重新安装 Daemon",
    fixCommand: "ghostcode init",
  },

  /** Daemon 权限不足 */
  BINARY_PERMISSION: {
    code: "GC_BINARY_002",
    category: ErrorCategory.BINARY,
    title: "Daemon 权限不足",
    description: "ghostcode-daemon 缺少执行权限",
    suggestion: "请运行 `chmod +x` 赋予执行权限，或重新运行 `ghostcode init`",
    fixCommand: "ghostcode init",
  },

  /** 版本不匹配 */
  VERSION_MISMATCH: {
    code: "GC_BINARY_003",
    category: ErrorCategory.BINARY,
    title: "版本不匹配",
    description: "Plugin 版本与 Daemon 版本不兼容",
    suggestion: "请运行 `ghostcode init` 更新 Daemon 到匹配版本",
    fixCommand: "ghostcode init",
  },

  // ----------------------------------------
  // 运行时错误（GC_RUNTIME_xxx）
  // Daemon 运行期间发生的故障
  // ----------------------------------------

  /** Daemon 崩溃 */
  DAEMON_CRASHED: {
    code: "GC_RUNTIME_001",
    category: ErrorCategory.RUNTIME,
    title: "Daemon 意外崩溃",
    description: "GhostCode Daemon 进程已崩溃或意外退出",
    suggestion: "请运行 `ghostcode doctor` 检查日志并重启 Daemon",
    fixCommand: "ghostcode doctor",
  },

  /** 会话过期 */
  SESSION_EXPIRED: {
    code: "GC_RUNTIME_002",
    category: ErrorCategory.RUNTIME,
    title: "会话已过期",
    description: "当前 GhostCode 会话租约已过期，需要重新建立连接",
    suggestion: "请重新启动 Claude Code 或运行 `ghostcode doctor`",
    fixCommand: "ghostcode doctor",
  },

  // ----------------------------------------
  // 网络错误（GC_NETWORK_xxx）
  // 下载 Daemon 二进制或远端资源时的网络故障
  // ----------------------------------------

  /** 下载失败 */
  DOWNLOAD_FAILED: {
    code: "GC_NETWORK_001",
    category: ErrorCategory.NETWORK,
    title: "下载失败",
    description: "下载 GhostCode Daemon 二进制文件时网络请求失败",
    suggestion: "请检查网络连接后重试，或手动下载并放置到正确路径",
  },

  /** Checksum 校验不通过 */
  CHECKSUM_MISMATCH: {
    code: "GC_NETWORK_002",
    category: ErrorCategory.NETWORK,
    title: "文件校验失败",
    description: "下载的 Daemon 文件 checksum 校验不通过，可能下载不完整或文件被篡改",
    suggestion: "请重新运行 `ghostcode init` 重新下载",
    fixCommand: "ghostcode init",
  },
};

// ============================================
// 未知错误回退模板（当 key 不在映射表中时使用）
// ============================================

/** 未知错误的回退模板 */
export const UNKNOWN_ERROR_TEMPLATE: ErrorTemplate = {
  code: "GC_UNKNOWN_000",
  category: ErrorCategory.UNKNOWN,
  title: "未知错误",
  description: "发生了未预期的错误",
  suggestion: "请运行 `ghostcode doctor` 查看详细诊断信息，或向开发者报告此问题",
  fixCommand: "ghostcode doctor",
};
