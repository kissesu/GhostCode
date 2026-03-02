//! 消息投递模块
//!
//! Phase 1 阶段 E 的核心模块，处理消息发送、接收和投递
//! - send.rs: 消息发送 + 事件写入（T11）
//! - inbox.rs: 收件箱读取 + 已读游标（T12）
//! - delivery.rs: 投递引擎（T13，待实现）
//!
//! 参考: cccc/src/cccc/daemon/messaging/ - 消息系统完整实现
//!
//! @author Atlas.oi
//! @date 2026-03-01

// ============================================
// 公共错误类型（send.rs 和 inbox.rs 共用）
// ============================================

/// 消息系统错误类型
#[derive(Debug, thiserror::Error)]
pub enum MessagingError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Ledger error: {0}")]
    Ledger(#[from] ghostcode_ledger::LedgerError),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("Group not found: {0}")]
    GroupNotFound(String),

    #[error("Sender not found: actor '{actor_id}' not in group '{group_id}'")]
    SenderNotFound { group_id: String, actor_id: String },

    #[error("Event not found: {0}")]
    EventNotFound(String),

    #[error("Invalid recipient: actor '{0}' not in group")]
    InvalidRecipient(String),
}

pub type Result<T> = std::result::Result<T, MessagingError>;

pub mod send;
pub mod inbox;
pub mod delivery;
