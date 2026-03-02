//! GhostCode 核心类型定义
//!
//! 包含 Event、DaemonRequest、DaemonResponse 等数据结构
//! 被所有其他 crate 依赖
//!
//! @author Atlas.oi
//! @date 2026-02-28

pub mod event;
pub mod ipc;
pub mod actor;
pub mod group;
pub mod addr;

#[cfg(any(test, feature = "testutil"))]
pub mod testutil;
