/**
 * @file cli.ts
 * @description GhostCode CLI 入口
 *   CLI 命令分发：init / 其他子命令
 *   通过 process.argv 手动解析参数，不引入额外 CLI 框架依赖
 * @author Atlas.oi
 * @date 2026-03-04
 */

import { runInitCommand } from "./cli/init.js";
import { runDoctor, formatDoctorReport } from "./cli/doctor.js";

// ============================================
// 版本信息
// ============================================

/** CLI 版本（从 package.json 读取） */
const VERSION = "0.1.0";

// ============================================
// 帮助信息
// ============================================

/**
 * 打印 CLI 帮助信息
 */
function printHelp(): void {
  console.log(`ghostcode v${VERSION}

用法：
  ghostcode <命令> [选项]

命令：
  init      初始化 GhostCode 运行环境（目录、二进制、MCP 配置）
  doctor    诊断 GhostCode 运行环境健康状态
  help      显示此帮助信息
  version   显示版本信息

示例：
  ghostcode init              # 初始化环境
  ghostcode init --dry-run    # 模拟运行，不实际修改文件
  ghostcode doctor            # 运行环境诊断
`);
}

// ============================================
// 命令处理函数
// ============================================

/**
 * 处理 init 子命令
 *
 * 业务逻辑：
 * 1. 解析 --dry-run 标志
 * 2. 解析 --mcp-json 参数（自定义 .mcp.json 路径）
 * 3. 调用 runInitCommand 执行初始化
 * 4. 输出执行结果
 *
 * @param args init 子命令之后的参数列表
 */
async function handleInit(args: string[]): Promise<void> {
  // 解析 --dry-run 标志
  const dryRun = args.includes("--dry-run");

  // 解析 --mcp-json <path> 参数
  let mcpJsonPath: string | undefined;
  const mcpJsonIdx = args.indexOf("--mcp-json");
  if (mcpJsonIdx >= 0) {
    mcpJsonPath = args[mcpJsonIdx + 1];
  }

  console.log("[GhostCode] 开始初始化...");
  if (dryRun) {
    console.log("[GhostCode] 模拟运行模式（--dry-run），不会修改文件");
  }

  const result = await runInitCommand({
    dryRun,
    ...(mcpJsonPath !== undefined ? { mcpJsonPath } : {}),
  });

  if (!result.success) {
    console.error(`[GhostCode] 初始化失败：${result.error ?? "未知错误"}`);
    process.exit(1);
  }

  // 输出各步骤执行情况
  if (result.dirsCreated) {
    console.log("[GhostCode] 目录结构已创建：~/.ghostcode/ 和 ~/.ghostcode/bin/");
  } else {
    console.log("[GhostCode] 目录结构已存在，跳过创建");
  }

  if (result.binInstalled) {
    console.log("[GhostCode] 二进制文件安装完成");
  } else {
    console.log("[GhostCode] 二进制文件已存在，跳过安装");
  }

  if (result.mcpJsonWritten) {
    console.log("[GhostCode] .mcp.json 配置已更新");
  }

  console.log("[GhostCode] 初始化完成！");
}

/**
 * 处理 doctor 子命令
 *
 * 业务逻辑：
 * 1. 调用 runDoctor 执行所有诊断检查
 * 2. 调用 formatDoctorReport 格式化输出
 * 3. 总体状态为 FAIL 时以非零退出码退出
 */
async function handleDoctor(): Promise<void> {
  const report = await runDoctor();
  console.log(formatDoctorReport(report));

  if (report.overallStatus === "FAIL") {
    process.exit(1);
  }
}

// ============================================
// CLI 主入口
// ============================================

/**
 * CLI 主入口函数
 *
 * 业务逻辑：
 * 1. 解析 argv 获取子命令名称
 * 2. 根据命令名分发到对应处理函数
 * 3. 未知命令时输出帮助信息并以非零退出码退出
 *
 * @param argv 命令行参数（默认为 process.argv）
 */
export async function main(argv: string[] = process.argv): Promise<void> {
  // process.argv 格式为 ["node", "cli.js", "命令", "参数..."]
  // 取 index 2 起为实际命令和参数
  const args = argv.slice(2);
  const command = args[0];

  // 无命令时显示帮助
  if (!command || command === "help" || command === "--help" || command === "-h") {
    printHelp();
    return;
  }

  // 版本查询
  if (command === "version" || command === "--version" || command === "-v") {
    console.log(`ghostcode v${VERSION}`);
    return;
  }

  // init 子命令
  if (command === "init") {
    await handleInit(args.slice(1));
    return;
  }

  // doctor 子命令
  if (command === "doctor") {
    await handleDoctor();
    return;
  }

  // 未知命令
  console.error(`[GhostCode] 未知命令：${command}`);
  printHelp();
  process.exit(1);
}

// ============================================
// 脚本直接执行入口
// 当作为 node dist/cli.js 执行时触发
// ============================================

// 检测是否为直接执行（ESM 兼容方式）
const isMainModule =
  process.argv[1] !== undefined &&
  (process.argv[1].endsWith("cli.js") || process.argv[1].endsWith("cli.ts"));

if (isMainModule) {
  main().catch((err: unknown) => {
    const errMsg = err instanceof Error ? err.message : String(err);
    console.error(`[GhostCode] CLI 发生未预期错误：${errMsg}`);
    process.exit(1);
  });
}
