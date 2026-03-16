/**
 * @file cli.ts
 * @description GhostCode CLI 入口
 *   CLI 命令分发：init / 其他子命令
 *   通过 process.argv 手动解析参数，不引入额外 CLI 框架依赖
 * @author Atlas.oi
 * @date 2026-03-04
 */
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
declare function main(argv?: string[]): Promise<void>;

export { main };
