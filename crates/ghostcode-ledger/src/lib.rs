//! GhostCode append-only 事件账本
//!
//! NDJSON (Newline Delimited JSON) 格式的事件持久化存储
//! 提供追加写入、反向读取、全量迭代等操作
//! 使用 flock 写锁保证并发安全
//!
//! 参考: cccc/src/cccc/kernel/ledger.py - append-only 事件账本实现
//!
//! @author Atlas.oi
//! @date 2026-03-01

pub mod blob;

use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;

use fs2::FileExt;
use ghostcode_types::event::Event;

/// 账本错误类型
#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("IO 错误: {0}")]
    Io(#[from] io::Error),

    #[error("序列化错误: {0}")]
    Serialize(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, LedgerError>;

// ============================================
// 内部辅助函数：flock 锁管理
// ============================================

/// 在持有排他锁的状态下执行操作
///
/// 使用 fs2 的 flock 机制确保同一时间只有一个写者
/// 操作完成后自动释放锁（通过 File drop）
///
/// 业务逻辑：
/// 1. 打开或创建锁文件
/// 2. 获取排他锁（阻塞等待）
/// 3. 执行用户提供的操作
/// 4. 释放锁（File drop 时自动释放）
fn with_lock<T>(lock_path: &Path, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_path)?;
    lock_file.lock_exclusive()?;
    let result = f();
    // 锁通过 lock_file drop 时自动释放，无需显式 unlock
    drop(lock_file);
    result
}

// ============================================
// 公共 API
// ============================================

/// 追加事件到账本
///
/// 双重保护：flock 写锁 + 原子追加
/// 每个事件序列化为一行 JSON，以换行符结尾
///
/// 业务逻辑：
/// 1. 获取排他锁
/// 2. 以 append 模式打开账本文件（不存在则创建）
/// 3. 将事件序列化为 JSON 并写入一行
/// 4. 释放锁
///
/// @param ledger_path - 账本文件路径（.jsonl）
/// @param lock_path - 锁文件路径（.lock）
/// @param event - 要追加的事件
pub fn append_event(ledger_path: &Path, lock_path: &Path, event: &Event) -> Result<()> {
    with_lock(lock_path, || {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(ledger_path)?;
        let json = serde_json::to_string(event)?;
        writeln!(file, "{}", json)?;
        Ok(())
    })
}

/// 读取最后 N 行（二进制反向扫描）
///
/// 从文件末尾按 4KB 块反向读取，找到换行符分割
/// 避免读取整个文件，对大账本性能更优
///
/// @param ledger_path - 账本文件路径
/// @param n - 要读取的行数
/// @return 最后 N 个事件（按时间顺序排列，最旧在前）
pub fn read_last_lines(ledger_path: &Path, n: usize) -> Result<Vec<Event>> {
    if n == 0 {
        return Ok(Vec::new());
    }

    let mut file = File::open(ledger_path)?;
    let file_len = file.metadata()?.len();

    if file_len == 0 {
        return Ok(Vec::new());
    }

    // 反向扫描缓冲区大小：4KB
    const BUF_SIZE: u64 = 4096;
    let mut lines: Vec<String> = Vec::new();
    let mut remaining = file_len;
    // 使用字节级拼接避免 UTF-8 多字节字符跨块截断问题
    let mut trailing_bytes: Vec<u8> = Vec::new();

    while remaining > 0 && lines.len() < n {
        // 计算本次读取的起始位置和长度
        let read_size = remaining.min(BUF_SIZE);
        let start_pos = remaining - read_size;

        file.seek(SeekFrom::Start(start_pos))?;
        let mut buf = vec![0u8; read_size as usize];
        file.read_exact(&mut buf)?;

        // 在字节级别拼接，避免中间做 UTF-8 转换导致多字节字符截断
        let mut combined = buf;
        combined.append(&mut trailing_bytes);

        // 在字节层面查找换行符 b'\n'
        let combined_str = String::from_utf8_lossy(&combined);
        let parts: Vec<&str> = combined_str.split('\n').collect();

        // 最前面一部分可能是不完整的行，保留到下一轮（以字节形式）
        trailing_bytes = parts[0].as_bytes().to_vec();

        // 从后往前收集完整行（跳过空行）
        for part in parts[1..].iter().rev() {
            if !part.is_empty() && lines.len() < n {
                lines.push(part.to_string());
            }
        }

        remaining = start_pos;
    }

    // 如果还需要更多行且有遗留数据，它就是第一行
    if lines.len() < n && !trailing_bytes.is_empty() {
        let trailing_str = String::from_utf8_lossy(&trailing_bytes).to_string();
        if !trailing_str.is_empty() {
            lines.push(trailing_str);
        }
    }

    // lines 目前是逆序（最新在前），需要反转为正序
    lines.reverse();

    // 反序列化为 Event
    let events: Vec<Event> = lines
        .into_iter()
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect();

    Ok(events)
}

/// 全量迭代（逐行 JSON 解析）
///
/// 返回一个迭代器，逐行读取账本文件并解析为 Event
/// 损坏的行（非法 JSON）会被跳过（[ERR-1] 容错）
///
/// @param ledger_path - 账本文件路径
/// @return 事件迭代器，每个 item 是 Result<Event>
pub fn iter_events(ledger_path: &Path) -> Result<impl Iterator<Item = std::result::Result<Event, LedgerError>>> {
    let file = File::open(ledger_path)?;
    let reader = BufReader::new(file);

    Ok(reader.lines().filter_map(|line_result| {
        match line_result {
            Ok(line) => {
                if line.trim().is_empty() {
                    return None;
                }
                match serde_json::from_str::<Event>(&line) {
                    Ok(event) => Some(Ok(event)),
                    // [ERR-1] 损坏行跳过，不 panic
                    Err(_) => None,
                }
            }
            Err(e) => Some(Err(LedgerError::Io(e))),
        }
    }))
}

/// 统计事件数量
///
/// 逐行扫描账本文件，统计有效 JSON 行数
/// 损坏行不计入统计
///
/// @param ledger_path - 账本文件路径
/// @return 有效事件数量
pub fn count_events(ledger_path: &Path) -> Result<usize> {
    if !ledger_path.exists() {
        return Ok(0);
    }

    let file = File::open(ledger_path)?;
    let reader = BufReader::new(file);
    let mut count = 0;
    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        if serde_json::from_str::<Event>(&line).is_ok() {
            count += 1;
        }
    }

    Ok(count)
}
