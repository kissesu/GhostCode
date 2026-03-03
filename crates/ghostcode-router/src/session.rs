// @file session.rs
// @description SESSION_ID 持久化管理，支持按 (group_id, actor_id, backend) 存取会话 ID
//              使用 RwLock 保证线程安全，JSON 格式持久化到文件
// @author Atlas.oi
// @date 2026-03-02

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;
use thiserror::Error;

/// Session 键：(group_id, actor_id, backend_name)
/// 三元组唯一标识一个 AI 后端会话，确保不同后端的会话不会互相干扰
pub type SessionKey = (String, String, String);

/// Session 存储错误类型
///
/// 使用 thiserror 自动实现 Display 和 Error trait，避免样板代码
#[derive(Debug, Error)]
pub enum SessionError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 序列化/反序列化错误: {0}")]
    Json(#[from] serde_json::Error),
    #[error("锁中毒错误: {0}")]
    Poison(String),
}

/// Session 存储（内存 + 文件持久化）
///
/// 设计原则：
/// - 内存优先：所有读写操作先操作内存，避免频繁 IO 影响性能
/// - 写后即存：save() 操作完成后立即 flush，保证数据不因进程崩溃丢失
/// - 线程安全：使用 RwLock 允许多读单写，适合高并发场景
pub struct SessionStore {
    /// 内存中的 session 映射，key 为 "group_id/actor_id/backend" 格式的复合键
    sessions: RwLock<HashMap<String, String>>,
    /// 持久化文件路径，用于跨进程/跨会话保留 session 数据
    file_path: PathBuf,
}

/// 将 SessionKey 三元组序列化为字符串键
///
/// 使用 "/" 作为分隔符，格式为 "group_id/actor_id/backend"
/// 选择 "/" 而非其他分隔符是因为它在 session_id 场景中很少出现
fn encode_key(key: &SessionKey) -> String {
    format!("{}/{}/{}", key.0, key.1, key.2)
}

/// 将字符串键反序列化为 SessionKey 三元组
///
/// 按照 "/" 分割，最多分成 3 段（group_id, actor_id, backend）
fn decode_key(s: &str) -> Option<SessionKey> {
    // 限制分割最多 3 部分，允许 backend_name 本身包含 "/"（虽然目前不会）
    let parts: Vec<&str> = s.splitn(3, '/').collect();
    if parts.len() == 3 {
        Some((parts[0].to_string(), parts[1].to_string(), parts[2].to_string()))
    } else {
        None
    }
}

impl SessionStore {
    /// 创建新的 SessionStore
    ///
    /// 如果 file_path 对应文件存在，则从文件加载已有 session 数据；
    /// 如果文件不存在，则初始化空的内存 store，等待后续 flush 创建文件。
    /// 这种惰性创建策略避免了不必要的空文件写入。
    pub fn new(file_path: PathBuf) -> Result<Self, SessionError> {
        // 尝试从文件加载已有数据，文件不存在时返回空 HashMap
        let sessions = if file_path.exists() {
            let content = std::fs::read_to_string(&file_path)?;
            serde_json::from_str::<HashMap<String, String>>(&content)?
        } else {
            HashMap::new()
        };

        Ok(Self {
            sessions: RwLock::new(sessions),
            file_path,
        })
    }

    /// 保存 session_id，已存在则覆盖旧值
    ///
    /// 业务逻辑：
    /// 1. 获取写锁更新内存中的映射
    /// 2. 立即 flush 到文件，保证持久化
    ///
    /// 写后即存的策略保证即使进程崩溃也不丢失最新 session 数据
    pub fn save(&self, key: SessionKey, session_id: String) -> Result<(), SessionError> {
        // 使用写锁更新内存中的 session 映射
        {
            let mut map = self.sessions.write().map_err(|e| SessionError::Poison(e.to_string()))?;
            map.insert(encode_key(&key), session_id);
        }
        // 立即持久化，避免数据仅存在于内存中
        self.flush()
    }

    /// 获取指定 key 对应的 session_id
    ///
    /// 使用读锁，允许多个线程并发读取，不阻塞其他读操作
    pub fn get(&self, key: &SessionKey) -> Option<String> {
        let map = self.sessions.read().ok()?;
        map.get(&encode_key(key)).cloned()
    }

    /// 列出所有已保存的 session 条目
    ///
    /// 将内部的字符串键解码回 SessionKey 三元组，过滤掉格式不合法的条目
    /// 主要用于调试、监控和管理面板展示
    pub fn list(&self) -> Vec<(SessionKey, String)> {
        let map = match self.sessions.read() {
            Ok(m) => m,
            Err(_) => return vec![],
        };
        map.iter()
            .filter_map(|(k, v)| decode_key(k).map(|key| (key, v.clone())))
            .collect()
    }

    /// 将内存中的 session 数据刷新到文件
    ///
    /// 业务逻辑：
    /// 1. 获取读锁，序列化 HashMap 为 JSON
    /// 2. 先写入临时文件（与目标文件同目录）
    /// 3. 原子重命名替换目标文件，防止写入一半时进程崩溃导致文件损坏
    pub fn flush(&self) -> Result<(), SessionError> {
        let map = self.sessions.read().map_err(|e| SessionError::Poison(e.to_string()))?;
        let content = serde_json::to_string_pretty(&*map)?;

        // 原子写入：先写临时文件再重命名，防止写入过程中崩溃导致数据损坏
        let tmp_path = self.file_path.with_extension("tmp");
        std::fs::write(&tmp_path, content)?;
        std::fs::rename(&tmp_path, &self.file_path)?;

        Ok(())
    }
}
