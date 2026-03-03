//! SSE 账本 tail 实现
//!
//! 将 NDJSON 账本文件的追加内容转化为 SSE 事件流
//! 参考: cccc/src/cccc/ports/web/streams.py sse_jsonl_tail 实现
//!
//! @author Atlas.oi
//! @date 2026-03-03

use std::path::PathBuf;
use std::time::Duration;

use tokio::io::AsyncBufReadExt;
use tokio_stream::Stream;

/// SSE 事件
///
/// 从账本 NDJSON 行映射而来，每行一个 SSE 事件
#[derive(Debug)]
pub struct SseEvent {
    /// 事件名称（固定为 "ledger"）
    pub event: String,
    /// 事件数据（原始 JSON 行）
    pub data: String,
}

/// 将账本文件 tail 转化为 SSE 事件流
///
/// 业务逻辑：
/// 1. 若 from_start=true，从文件开头读取所有历史事件
/// 2. 若 from_start=false，seek 到文件末尾只推送新事件
/// 3. 每行 NDJSON 映射为一个 SSE 事件
/// 4. 文件不存在时等待创建（每 200ms 轮询，最多 10 次）
///
/// @param ledger_path - 账本文件路径
/// @param from_start - 是否从头开始读取
/// @returns 异步 SSE 事件流
pub fn tail_ledger_as_sse(
    ledger_path: PathBuf,
    from_start: bool,
) -> impl Stream<Item = SseEvent> {
    async_stream::stream! {
        // 等待文件出现（最多等待 10 次，每次 200ms）
        let mut retries = 0;
        loop {
            if ledger_path.exists() {
                break;
            }
            if retries >= 10 {
                return; // 文件长时间不存在，退出流
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
            retries += 1;
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

        // 持续 tail：循环读取新行
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
