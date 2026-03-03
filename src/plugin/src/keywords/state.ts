/**
 * @file state.ts
 * @description Magic Keywords 状态持久化管理
 * 提供关键词激活状态的原子读写功能，状态存储在 .ghostcode/state/keywords.json
 * @author Atlas.oi
 * @date 2026-03-03
 */

import * as fs from "node:fs/promises";
import * as path from "node:path";
import type { KeywordState } from "./types.js";

// 状态文件相对于工作区根目录的路径
const STATE_FILE_RELATIVE_PATH = path.join(
  ".ghostcode",
  "state",
  "keywords.json"
);

/**
 * 默认关键词状态
 * 文件不存在时返回此默认值
 */
const DEFAULT_STATE: KeywordState = {
  active: null,
  activatedAt: null,
  prompt: null,
};

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
export async function readKeywordState(
  workspaceRoot: string
): Promise<KeywordState> {
  const statePath = path.join(workspaceRoot, STATE_FILE_RELATIVE_PATH);

  try {
    const content = await fs.readFile(statePath, "utf-8");
    return JSON.parse(content) as KeywordState;
  } catch (err) {
    // 文件不存在时返回默认状态
    if ((err as NodeJS.ErrnoException).code === "ENOENT") {
      return { ...DEFAULT_STATE };
    }
    // 其他错误（权限、JSON 解析错误等）直接抛出，暴露问题而非降级
    throw err;
  }
}

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
export async function writeKeywordState(
  workspaceRoot: string,
  state: KeywordState
): Promise<void> {
  const statePath = path.join(workspaceRoot, STATE_FILE_RELATIVE_PATH);
  const stateDir = path.dirname(statePath);

  // 递归创建目录层级（如 .ghostcode/state/）
  await fs.mkdir(stateDir, { recursive: true });

  // 序列化为格式化 JSON，便于人工查看
  const content = JSON.stringify(state, null, 2);

  // 直接写入（覆盖），实现幂等性
  await fs.writeFile(statePath, content, "utf-8");
}
