/**
 * @file postinstall.ts
 * @description npm/pnpm install 后自动触发的二进制安装脚本
 *              支持 CI 环境检测、GitHub Release 网络下载
 *
 * 业务流程：
 * 1. 检测 CI 环境 -> 跳过所有下载操作，仅输出提示
 * 2. 尝试 installFromRelease 下载最新版本
 * 3. 下载失败 -> 输出明确错误信息、失败原因和修复建议
 * 4. 权限错误 -> 输出 chmod/sudo 相关提示
 *
 * @author Atlas.oi
 * @date 2026-03-04
 */
/**
 * 检测是否为 CI 环境
 *
 * 业务逻辑：
 * 检查常见 CI 平台的环境变量标识
 * - CI: GitHub Actions, CircleCI, Travis CI 等通用标识
 * - GITHUB_ACTIONS: GitHub Actions 专用
 * - JENKINS_URL: Jenkins CI
 * - GITLAB_CI: GitLab CI
 *
 * @returns true 表示当前运行在 CI 环境中
 */
declare function isCIEnvironment(): boolean;
/**
 * postinstall 主入口函数
 *
 * 在 npm/pnpm install 完成后自动执行。
 * 输出保持简洁，不干扰包管理器的主控制台输出。
 *
 * 业务逻辑：
 * 1. CI 环境检测 -> 跳过下载
 * 2. installFromRelease 下载最新 bundle
 * 3. 下载失败 -> 直接报错，输出失败原因和修复建议（禁止降级回退）
 * 4. 权限错误 -> 输出 chmod/sudo 建议
 */
declare function runPostinstall(): Promise<void>;

export { isCIEnvironment, runPostinstall };
