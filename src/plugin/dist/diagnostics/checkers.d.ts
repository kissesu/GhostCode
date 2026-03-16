/**
 * @file 诊断检查器集合
 * @description 插件化检查器模式，每个检查器独立检测一个维度的健康状态。
 *              涵盖：daemon 二进制存在性、Node.js 版本、daemon 可达性、
 *              plugin/daemon 版本匹配、配置文件有效性。
 * @author Atlas.oi
 * @date 2026-03-04
 */
/** 检查状态：PASS 正常 / FAIL 失败 / WARN 警告 */
type CheckStatus = "PASS" | "FAIL" | "WARN";
/** 单个检查项的结果 */
interface CheckResult {
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
interface Checker {
    /** 检查项名称 */
    name: string;
    /** 执行检查并返回结果 */
    run: () => Promise<CheckResult>;
}
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
declare function checkBinaryPath(binaryPath?: string): Promise<CheckResult>;
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
declare function checkNodeVersion(version?: string): CheckResult;
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
declare function checkDaemonReachable(): Promise<CheckResult>;
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
declare function checkVersionMatch(pluginVersion?: string, daemonVersion?: string): Promise<CheckResult>;
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
declare function checkConfigValid(): Promise<CheckResult>;

export { type CheckResult, type CheckStatus, type Checker, checkBinaryPath, checkConfigValid, checkDaemonReachable, checkNodeVersion, checkVersionMatch };
