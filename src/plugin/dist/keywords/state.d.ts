import { KeywordState } from './types.js';

/**
 * @file state.ts
 * @description Magic Keywords 状态持久化管理
 * 提供关键词激活状态的原子读写功能，状态存储在 .ghostcode/state/keywords.json
 * @author Atlas.oi
 * @date 2026-03-03
 */

/**
 * 读取关键词激活状态
 *
 * 业务逻辑：
 * 1. 构建状态文件的完整路径
 * 2. 尝试读取并解析 JSON 文件
 * 3. 如果文件不存在（ENOENT），返回默认状态
 * 4. 其他错误继续抛出（不使用降级策略，暴露真实问题）
 *
 * @param workspaceRoot - 工作区根目录的绝对路径
 * @returns 关键词状态对象
 */
declare function readKeywordState(workspaceRoot: string): Promise<KeywordState>;
/**
 * 写入关键词激活状态（原子写入）
 *
 * 业务逻辑：
 * 1. 构建状态文件的完整路径
 * 2. 递归创建目录（如不存在）
 * 3. 序列化状态为格式化 JSON
 * 4. 写入文件（直接覆盖，实现幂等性）
 *
 * @param workspaceRoot - 工作区根目录的绝对路径
 * @param state - 要持久化的关键词状态
 */
declare function writeKeywordState(workspaceRoot: string, state: KeywordState): Promise<void>;

export { readKeywordState, writeKeywordState };
