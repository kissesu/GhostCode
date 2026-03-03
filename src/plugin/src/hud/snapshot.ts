/**
 * @file HUD 快照获取器
 * @description 通过 IPC 向 Rust Daemon 调用 hud_snapshot op，获取 HudSnapshot 数据
 *              使用 src/ipc.ts 中的 callDaemon 函数进行通信
 * @author Atlas.oi
 * @date 2026-03-03
 */
import { callDaemon } from "../ipc";
import type { HudSnapshot } from "./types";

/**
 * 从 Daemon 获取 HUD 状态快照
 *
 * 业务逻辑：
 * 1. 调用 callDaemon("hud_snapshot") 发起 IPC 请求
 * 2. 验证响应是否成功（ok: true）
 * 3. 将 result 转型为 HudSnapshot 返回
 *
 * @param socketPath - Unix socket 路径（可选，默认读取环境变量 GHOSTCODE_SOCKET_PATH）
 * @returns 解析后的 HudSnapshot 对象
 * @throws 当 Daemon 调用失败或响应格式不符时抛出错误
 */
export async function fetchHudSnapshot(socketPath?: string): Promise<HudSnapshot> {
  const response = await callDaemon("hud_snapshot", {}, socketPath);

  if (!response.ok) {
    const errorMsg = response.error
      ? `${response.error.code}: ${response.error.message}`
      : "未知错误";
    throw new Error(`hud_snapshot 调用失败: ${errorMsg}`);
  }

  // 将 result 转型为 HudSnapshot
  // Daemon 保证返回结构与 HudSnapshot 接口对齐
  return response.result as HudSnapshot;
}
