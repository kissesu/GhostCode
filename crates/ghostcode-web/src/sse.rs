//! SSE 账本 tail 实现
//!
//! 将 NDJSON 账本文件的追加内容转化为 SSE 事件流
//! 参考: cccc/src/cccc/ports/web/streams.py sse_jsonl_tail 实现
//!
//! @author Atlas.oi
//! @date 2026-03-05

use std::path::PathBuf;
use std::time::Duration;

use tokio::io::AsyncBufReadExt;
use tokio_stream::Stream;

/// SSE 事件
///
/// 从账本 NDJSON 行映射而来，每行一个 SSE 事件
/// 特殊事件：connected 心跳，用于冲刷代理缓冲区
#[derive(Debug)]
pub struct SseEvent {
    /// 事件名称（"ledger" 或 "heartbeat"）
    pub event: String,
    /// 事件数据（原始 JSON 行或心跳 JSON）
    pub data: String,
}

/// 将账本文件 tail 转化为 SSE 事件流
///
/// 业务逻辑：
/// 1. 立即发送 connected 心跳事件（冲刷 HTTP 代理缓冲区，让前端立即收到 onopen）
/// 2. 若 from_start=true，从文件开头读取所有历史事件
/// 3. 若 from_start=false，seek 到文件末尾只推送新事件
/// 4. 每行 NDJSON 映射为一个 SSE 事件
/// 5. 文件不存在时等待创建（每 2 秒轮询）
///
/// @param ledger_path - 账本文件路径
/// @param from_start - 是否从头开始读取
/// @returns 异步 SSE 事件流
pub fn tail_ledger_as_sse(
    ledger_path: PathBuf,
    from_start: bool,
) -> impl Stream<Item = SseEvent> {
    async_stream::stream! {
        // ============================================
        // 第一步：立即发送 connected 心跳
        // 解决 Vite/nginx 等 HTTP 代理缓冲 SSE 响应的问题：
        // 代理在收到第一个 body 数据前会缓冲响应头，
        // 导致前端 EventSource.onopen 延迟 10-15 秒
        // 发送一个初始事件即可冲刷缓冲区
        // ============================================
        yield SseEvent {
            event: "heartbeat".to_string(),
            data: r#"{"type":"connected"}"#.to_string(),
        };

        // ============================================
        // 第二步：等待账本文件出现（带超时保护）
        // 文件不存在时持续等待（每 2 秒轮询），最多等待 5 分钟
        // 5 分钟内文件一旦出现，最迟 2 秒后激活 SSE 流
        // 超时后优雅退出，前端 EventSource.onerror 触发自动重连
        //
        // 为什么是 5 分钟而非更长：
        // - 前端 EventSource 自带自动重连机制，超时后会重新建立连接
        // - 过长的超时（如 30 分钟）会导致多客户端并发连接不存在 group 时
        //   Tokio task 长期积累，浪费服务器资源
        // - 5 分钟足以覆盖 Daemon 冷启动 + 首次事件写入的延迟
        // ============================================
        const FILE_WAIT_TIMEOUT: Duration = Duration::from_secs(5 * 60);
        let wait_start = tokio::time::Instant::now();

        loop {
            if ledger_path.exists() {
                break;
            }
            if wait_start.elapsed() > FILE_WAIT_TIMEOUT {
                // 超时退出：文件 30 分钟未出现，避免资源泄漏
                // 前端 EventSource 会自动重连，届时再次检查
                return;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        // 打开文件
        let file = match tokio::fs::File::open(&ledger_path).await {
            Ok(f) => f,
            Err(_) => return,
        };

        let mut reader = tokio::io::BufReader::new(file);

        // 若不从头读，skip 到文件末尾（读取并丢弃所有现有行）
        if !from_start {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // 到达 EOF
                    Ok(_) => continue,
                    Err(_) => return,
                }
            }
        }

        // ============================================
        // 第三步：持续 tail，循环读取新行
        // ============================================
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    // 暂时 EOF，等待新数据写入
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    continue;
                }
                Ok(_) => {
                    let trimmed = line.trim().to_string();
                    if !trimmed.is_empty() {
                        yield SseEvent {
                            event: "ledger".to_string(),
                            data: trimmed,
                        };
                    }
                }
                Err(_) => break,
            }
        }
    }
}
