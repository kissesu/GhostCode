/**
 * @file init.ts
 * @description ghostcode init 子命令
 *   初始化 GhostCode 运行环境：目录结构、二进制安装、MCP 配置
 * @author Atlas.oi
 * @date 2026-03-04
 */
/**
 * init 命令选项
 */
interface InitOptions {
    /**
     * 测试模式：不执行真实的 fs 写入和网络下载，只返回会执行的操作描述
     * 主要用于单元测试中验证逻辑分支
     */
    dryRun?: boolean;
    /** .mcp.json 写入路径，默认为 process.cwd()/.mcp.json */
    mcpJsonPath?: string;
}
/**
 * init 命令执行结果
 */
interface InitResult {
    /** 总体成功标志 */
    success: boolean;
    /** 是否创建了目录 */
    dirsCreated: boolean;
    /** 是否触发了二进制安装 */
    binInstalled: boolean;
    /** 是否写入/更新了 .mcp.json */
    mcpJsonWritten: boolean;
    /** 错误信息（失败时非空） */
    error?: string;
}
/**
 * 执行 ghostcode init 命令
 *
 * 业务逻辑：
 * 1. ensureDirs() — 创建 ~/.ghostcode/ 和 ~/.ghostcode/bin/
 * 2. ensureBinaries() — 检查二进制是否存在，不存在则调用 installFromRelease
 * 3. ensureMcpConfig() — 生成/合并 .mcp.json（幂等操作）
 *
 * 幂等保证：
 * - 目录已存在时不报错（mkdirSync recursive）
 * - 二进制已存在时跳过安装（isInstalledInDir 检查）
 * - .mcp.json 已有 ghostcode 配置时执行更新而非重复添加
 *
 * @param options init 命令选项
 * @returns 执行结果对象
 */
declare function runInitCommand(options?: InitOptions): Promise<InitResult>;

export { type InitOptions, type InitResult, runInitCommand };
