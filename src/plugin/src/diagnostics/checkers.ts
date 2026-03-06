/**
 * @file 诊断检查器集合
 * @description 插件化检查器模式，每个检查器独立检测一个维度的健康状态。
 *              涵盖：daemon 二进制存在性、Node.js 版本、daemon 可达性、
 *              plugin/daemon 版本匹配、配置文件有效性。
 * @author Atlas.oi
 * @date 2026-03-04
 */

import * as fs from "node:fs/promises";
import * as net from "node:net";
import * as os from "node:os";
import * as path from "node:path";

// ============================================
// 对外类型定义
// ============================================

/** 检查状态：PASS 正常 / FAIL 失败 / WARN 警告 */
export type CheckStatus = "PASS" | "FAIL" | "WARN";

/** 单个检查项的结果 */
export interface CheckResult {
  /** 检查项名称，用于 CLI 输出标识 */
  name: string;
  /** 检查状态 */
  status: CheckStatus;
  /** 描述本次检查结果的消息 */
  message: string;
  /** 修复建议（可选），当 status 为 FAIL 或 WARN 时提供 */
  suggestion?: string | undefined;
}

/** 检查器接口：每个检查器持有名称和异步运行方法 */
export interface Checker {
  /** 检查项名称 */
  name: string;
  /** 执行检查并返回结果 */
  run: () => Promise<CheckResult>;
}

// ============================================
// daemon 二进制路径常量
// 安装路径为 ~/.ghostcode/bin/ghostcoded
// ============================================

/** 默认 daemon 二进制路径 */
const DEFAULT_BINARY_PATH = path.join(os.homedir(), ".ghostcode", "bin", "ghostcoded");

/**
 * 检查 daemon 二进制是否存在于 ~/.ghostcode/bin/ghostcoded
 *
 * 业务逻辑：
 * 1. 尝试访问固定路径的 daemon 二进制文件
 * 2. 访问成功则 PASS，文件不存在（ENOENT）则 FAIL
 *
 * @param binaryPath - 可覆盖的二进制路径，默认为 ~/.ghostcode/bin/ghostcoded
 * @returns 检查结果
 */
export async function checkBinaryPath(binaryPath?: string): Promise<CheckResult> {
  // 使用传入路径或默认路径
  const targetPath = binaryPath ?? DEFAULT_BINARY_PATH;

  try {
    // 尝试访问文件，验证其存在性
    await fs.access(targetPath);

    return {
      name: "binary",
      status: "PASS",
      message: `ghostcoded 二进制存在于 ${targetPath}`,
    };
  } catch {
    // 文件不存在或无法访问
    return {
      name: "binary",
      status: "FAIL",
      message: `找不到 ghostcoded 二进制文件，路径：${targetPath}`,
      suggestion: "请运行 `ghostcode init` 重新安装 Daemon",
    };
  }
}

/**
 * 检查 Node.js 版本是否满足最低要求 >= 20
 *
 * 业务逻辑：
 * 1. 解析版本字符串，提取主版本号
 * 2. 主版本号 >= 20 则 PASS，否则 FAIL
 *
 * @param version - 可覆盖的版本字符串，默认使用 process.version（如 'v20.10.0'）
 * @returns 检查结果（同步）
 */
export function checkNodeVersion(version?: string): CheckResult {
  // 使用传入版本或当前进程版本
  const versionStr = version ?? process.version;

  // 从 'v20.10.0' 格式中提取主版本号
  // 去掉开头的 'v'，再取第一个点之前的部分
  const majorStr = versionStr.replace(/^v/, "").split(".")[0];
  const major = majorStr !== undefined ? parseInt(majorStr, 10) : 0;

  if (major >= 20) {
    return {
      name: "node-version",
      status: "PASS",
      message: `Node.js 版本 ${versionStr} 满足最低要求 (>= 20)`,
    };
  }

  return {
    name: "node-version",
    status: "FAIL",
    message: `Node.js 版本 ${versionStr} 低于最低要求，需要 >= 20`,
    suggestion: "请升级 Node.js 到 20 或更高版本，推荐使用 volta 管理版本",
  };
}

/**
 * 检查 daemon 是否可达（尝试连接 Unix socket）
 *
 * 业务逻辑：
 * 1. 读取 ~/.ghostcode/addr.json 获取 socket 路径
 * 2. 尝试 TCP/Unix socket 连接，2 秒超时
 * 3. 连接成功则 PASS，超时或连接拒绝则 FAIL
 *
 * @returns 检查结果
 */
export async function checkDaemonReachable(): Promise<CheckResult> {
  // ============================================
  // 步骤 1：读取 addr.json 获取 socket 路径
  // ============================================
  // 路径与 Rust Daemon 的 DaemonPaths 保持一致：
  // daemon 写入 base_dir/daemon/ghostcoded.addr.json
  const addrPath = path.join(os.homedir(), ".ghostcode", "daemon", "ghostcoded.addr.json");

  let socketPath: string;
  try {
    const content = await fs.readFile(addrPath, "utf-8");
    const parsed = JSON.parse(content) as Record<string, unknown>;
    // 优先读取新版 path 字段，兼容旧版 socket_path 字段（过渡期）
    // 字段契约：addr.json 必须包含 path 或 socket_path 之一
    const rawPath = parsed["path"] ?? parsed["socket_path"];
    if (typeof rawPath !== "string" || rawPath.length === 0) {
      return {
        name: "daemon-reachable",
        status: "FAIL",
        message: "addr.json 中 path 字段无效或缺失（字段契约：需要 path 或 socket_path）",
        suggestion: "请运行 `ghostcode init` 重启 Daemon 以写入正确的 addr.json",
      };
    }
    socketPath = rawPath;
  } catch {
    return {
      name: "daemon-reachable",
      status: "FAIL",
      message: "无法读取 addr.json，Daemon 可能未启动",
      suggestion: "请运行 `ghostcode init` 启动 Daemon",
    };
  }

  // ============================================
  // 步骤 2：尝试连接 Unix socket，2 秒超时
  // ============================================
  return new Promise<CheckResult>((resolve) => {
    const socket = net.createConnection(socketPath);
    let resolved = false;

    // 超时处理：2 秒内未连接则视为不可达
    const timeout = setTimeout(() => {
      if (!resolved) {
        resolved = true;
        socket.destroy();
        resolve({
          name: "daemon-reachable",
          status: "FAIL",
          message: `连接 Daemon socket 超时（路径：${socketPath}）`,
          suggestion: "请运行 `ghostcode doctor` 检查 Daemon 状态",
        });
      }
    }, 2000);

    socket.on("connect", () => {
      if (!resolved) {
        resolved = true;
        clearTimeout(timeout);
        socket.destroy();
        resolve({
          name: "daemon-reachable",
          status: "PASS",
          message: `成功连接到 Daemon（路径：${socketPath}）`,
        });
      }
    });

    socket.on("error", (err) => {
      if (!resolved) {
        resolved = true;
        clearTimeout(timeout);
        resolve({
          name: "daemon-reachable",
          status: "FAIL",
          message: `无法连接到 Daemon：${err.message}`,
          suggestion: "请运行 `ghostcode doctor` 检查 Daemon 状态",
        });
      }
    });
  });
}

/**
 * 检查 plugin 和 daemon 版本是否匹配
 *
 * 业务逻辑：
 * 1. 获取 plugin 版本（来自 package.json 或传入参数）
 * 2. 获取 daemon 版本（来自传入参数或读取 ~/.ghostcode/daemon-version 文件）
 * 3. 比较版本字符串，不匹配则 FAIL
 *
 * @param pluginVersion - 可覆盖的 plugin 版本字符串
 * @param daemonVersion - 可覆盖的 daemon 版本字符串
 * @returns 检查结果
 */
export async function checkVersionMatch(
  pluginVersion?: string,
  daemonVersion?: string
): Promise<CheckResult> {
  // ============================================
  // 步骤 1：获取 plugin 版本
  // 优先使用传入参数，避免硬编码或 package.json 读取的复杂性
  // ============================================
  // 从 package.json 读取真实 plugin 版本，避免硬编码 "unknown"
  let pVer: string;
  if (pluginVersion !== undefined) {
    pVer = pluginVersion;
  } else {
    try {
      const pkgPath = path.join(path.dirname(new URL(import.meta.url).pathname), "..", "..", "package.json");
      const pkgContent = await fs.readFile(pkgPath, "utf-8");
      const pkg = JSON.parse(pkgContent) as { version?: string };
      pVer = pkg.version ?? "unknown";
    } catch {
      pVer = "unknown";
    }
  }

  // ============================================
  // 步骤 2：获取 daemon 版本
  // 如果未传入则从 addr.json 的 version 字段读取（Daemon 启动时写入）
  // 路径与 Rust DaemonPaths 一致：base_dir/daemon/ghostcoded.addr.json
  // ============================================
  let dVer: string;
  if (daemonVersion !== undefined) {
    dVer = daemonVersion;
  } else {
    const addrPath = path.join(os.homedir(), ".ghostcode", "daemon", "ghostcoded.addr.json");
    try {
      const content = await fs.readFile(addrPath, "utf-8");
      const parsed = JSON.parse(content) as Record<string, unknown>;
      const rawVersion = parsed["version"];
      if (typeof rawVersion !== "string" || rawVersion.length === 0) {
        return {
          name: "version-match",
          status: "WARN",
          message: "addr.json 中缺少 version 字段，跳过版本匹配检查",
          suggestion: "请运行 `ghostcode init` 确保 Daemon 正确安装",
        };
      }
      dVer = rawVersion;
    } catch {
      return {
        name: "version-match",
        status: "WARN",
        message: "无法读取 Daemon 版本信息（Daemon 可能未启动），跳过版本匹配检查",
        suggestion: "请运行 `ghostcode init` 确保 Daemon 正确安装",
      };
    }
  }

  // ============================================
  // 步骤 3：版本比较
  // 使用字符串精确匹配，主版本必须一致
  // ============================================
  if (pVer === dVer) {
    return {
      name: "version-match",
      status: "PASS",
      message: `Plugin 版本 (${pVer}) 与 Daemon 版本 (${dVer}) 匹配`,
    };
  }

  return {
    name: "version-match",
    status: "FAIL",
    message: `版本不匹配：Plugin ${pVer} vs Daemon ${dVer}`,
    suggestion: "请运行 `ghostcode init` 更新 Daemon 到匹配版本",
  };
}

/**
 * 检查配置文件格式是否有效
 *
 * 业务逻辑：
 * 1. 读取 ~/.ghostcode/config.toml
 * 2. 验证文件存在且非空
 * 3. 注意：TOML 解析依赖 Rust Daemon 完成，这里仅做存在性检查
 *
 * @returns 检查结果
 */
export async function checkConfigValid(): Promise<CheckResult> {
  const configPath = path.join(os.homedir(), ".ghostcode", "config.toml");

  try {
    const stat = await fs.stat(configPath);

    // 文件必须有内容，空配置文件无意义
    if (stat.size === 0) {
      return {
        name: "config",
        status: "WARN",
        message: `配置文件存在但为空：${configPath}`,
        suggestion: "请运行 `ghostcode init` 生成默认配置",
      };
    }

    return {
      name: "config",
      status: "PASS",
      message: `配置文件存在且非空：${configPath}`,
    };
  } catch {
    return {
      name: "config",
      status: "FAIL",
      message: `配置文件不存在：${configPath}`,
      suggestion: "请运行 `ghostcode init` 初始化配置",
    };
  }
}
