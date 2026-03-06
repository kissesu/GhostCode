/**
 * @file scripts/lib/daemon-client.mjs
 * @description Daemon Unix Socket IPC 客户端
 *              供所有 Hook 脚本共用的 Daemon 通信工具库。
 *
 *              通信协议：
 *              1. 读取 ~/.ghostcode/daemon/ghostcoded.addr.json 获取 socket 路径
 *              2. 发送 NDJSON 请求：{"v":1,"op":"<op>","args":{...}}\n
 *              3. 读取一行 JSON 响应
 *              4. 超时 2000ms，addr.json 不存在时返回 null
 *
 *              参考: ghostcode-types/src/ipc.rs - DaemonRequest/DaemonResponse 协议定义
 * @author Atlas.oi
 * @date 2026-03-06
 */

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";
import { createConnection } from "node:net";

// ============================================
// 常量配置
// ============================================

// GhostCode 主目录，支持环境变量覆盖（主要用于测试隔离）
const GHOSTCODE_HOME = process.env.GHOSTCODE_HOME || join(homedir(), ".ghostcode");

// Daemon 地址文件路径（Daemon 启动时写入，包含 socket 路径）
const ADDR_FILE = join(GHOSTCODE_HOME, "daemon", "ghostcoded.addr.json");

// IPC 请求超时时间（毫秒）
// 设为 2000ms，与 stdin.mjs 的超时对齐，确保在 Hook 外层超时（5s）前完成
const IPC_TIMEOUT_MS = 2000;

// ============================================
// 核心 IPC 函数
// ============================================

/**
 * 向 Daemon 发送 IPC 请求并等待响应
 *
 * 业务逻辑说明：
 * 1. 读取 addr.json 获取 Unix Socket 路径
 * 2. 建立 Socket 连接
 * 3. 发送 NDJSON 格式请求（一行 JSON + 换行符）
 * 4. 读取一行 JSON 响应
 * 5. 解析并返回 DaemonResponse
 *
 * addr.json 不存在或 Socket 连接失败时返回 null（Daemon 未运行）
 * Hook 脚本调用方应检查返回值是否为 null
 *
 * @param {string} op - 操作名称（如 "actor_start", "actor_stop", "send", "ping"）
 * @param {object} args - 操作参数（任意 JSON 对象）
 * @returns {Promise<object|null>} DaemonResponse 对象（{v, ok, result, error}），或 null（Daemon 不可达）
 *
 * @example
 * const resp = await callDaemon("actor_start", { group_id: "g-xxx", actor_id: "agent-1" });
 * if (resp?.ok) {
 *   console.log("Actor 注册成功");
 * }
 */
export async function callDaemon(op, args = {}) {
  // ============================================
  // 第一步：读取 Daemon 地址文件
  // addr.json 由 Daemon 启动时写入，包含 socket 路径
  // 不存在 = Daemon 未运行，返回 null
  // ============================================
  let socketPath;
  try {
    if (!existsSync(ADDR_FILE)) {
      return null;
    }
    const addrData = JSON.parse(readFileSync(ADDR_FILE, "utf-8"));
    socketPath = addrData.path;
    if (!socketPath) {
      return null;
    }
  } catch {
    // addr.json 读取/解析失败，视为 Daemon 不可达
    return null;
  }

  // ============================================
  // 第二步：构造 NDJSON 请求
  // 协议格式：{"v":1,"op":"<op>","args":{...}}\n
  // 参考: ghostcode-types/src/ipc.rs:21 - DaemonRequest 结构
  // ============================================
  const request = JSON.stringify({ v: 1, op, args }) + "\n";

  // ============================================
  // 第三步：通过 Unix Socket 发送请求并接收响应
  // 使用 Promise 包裹 net.createConnection，带超时保护
  // ============================================
  return new Promise((resolve) => {
    let settled = false;
    let responseData = "";

    /**
     * 安全 resolve：防止多次 resolve（超时 + 正常响应竞争）
     *
     * @param {object|null} value - 要 resolve 的值
     */
    function safeResolve(value) {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      socket.destroy();
      resolve(value);
    }

    // 创建 Unix Socket 连接
    const socket = createConnection({ path: socketPath }, () => {
      // 连接建立后发送请求
      socket.write(request);
    });

    socket.setEncoding("utf-8");

    // 累积响应数据，遇到换行符表示完整响应到达
    socket.on("data", (chunk) => {
      responseData += chunk;

      // NDJSON 协议：一行 = 一个完整响应
      const newlineIdx = responseData.indexOf("\n");
      if (newlineIdx !== -1) {
        const line = responseData.slice(0, newlineIdx).trim();
        try {
          const parsed = JSON.parse(line);
          safeResolve(parsed);
        } catch {
          // 响应解析失败，返回 null
          safeResolve(null);
        }
      }
    });

    // 连接错误（Daemon 已停止、socket 文件残留等）
    socket.on("error", () => {
      safeResolve(null);
    });

    // 连接关闭但未收到完整响应
    socket.on("end", () => {
      if (!settled) {
        // 尝试解析已收到的数据（可能没有换行符结尾）
        const trimmed = responseData.trim();
        if (trimmed) {
          try {
            safeResolve(JSON.parse(trimmed));
            return;
          } catch {
            // 解析失败，返回 null
          }
        }
        safeResolve(null);
      }
    });

    // 超时保护：2000ms 后若仍未收到响应，返回 null
    const timer = setTimeout(() => {
      safeResolve(null);
    }, IPC_TIMEOUT_MS);
  });
}
