/**
 * @file scripts/lib/stdin.mjs
 * @description stdin 读取共享工具库
 *              提供带超时防抖的 readStdin 函数，供所有 Hook 脚本共用。
 *
 *              设计要点：
 *              1. settled 标志防止 Promise 被多次 resolve（end 事件 + 超时竞争）
 *              2. 超时设为 2000ms（小于 hooks.json 的 5s），留出余量
 *              3. end 触发后立即 clearTimeout，避免超时误触发
 *              4. 出错时 reject，调用方应 try-catch
 * @author Atlas.oi
 * @date 2026-03-05
 */

/**
 * 从 process.stdin 读取完整输入，带超时保护和防抖机制
 *
 * 业务逻辑说明：
 * 1. 监听 data 事件，累积所有分块数据
 * 2. 监听 end 事件，触发 resolve 并清除超时定时器
 * 3. 监听 error 事件，触发 reject
 * 4. 2000ms 超时兜底：即使 end 未到达，也返回已收到的数据
 * 5. settled 标志确保 Promise 只 resolve/reject 一次（防止竞争条件）
 *
 * @returns {Promise<string>} stdin 的完整文本内容
 */
export function readStdin() {
  return new Promise((resolve, reject) => {
    // 已结算标志：防止 end 事件和超时同时 resolve，导致回调执行两次
    let settled = false;

    // 累积的输入数据缓冲区
    let data = "";

    /**
     * 安全 resolve：检查 settled 标志后再 resolve
     * 同时清除超时定时器，防止后续误触发
     *
     * @param {string} value - 要 resolve 的值
     */
    function safeResolve(value) {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve(value);
    }

    /**
     * 安全 reject：检查 settled 标志后再 reject
     *
     * @param {Error} err - 要 reject 的错误
     */
    function safeReject(err) {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      reject(err);
    }

    // ============================================
    // 设置字符编码为 UTF-8，确保中文等多字节字符正确处理
    // ============================================
    process.stdin.setEncoding("utf-8");

    // 监听数据块：累积到 data 缓冲区
    process.stdin.on("data", (chunk) => {
      data += chunk;
    });

    // 监听 end 事件：stdin 关闭，返回完整数据
    process.stdin.on("end", () => {
      safeResolve(data);
    });

    // 监听 error 事件：stdin 读取出错，reject Promise
    process.stdin.on("error", (err) => {
      safeReject(err);
    });

    // ============================================
    // 超时兜底：2000ms 后若 Promise 仍未结算，返回已收到的数据
    // 设为 2000ms 而非 hooks.json 的 5s，确保在外层超时前完成
    // ============================================
    const timer = setTimeout(() => {
      safeResolve(data);
    }, 2000);
  });
}
