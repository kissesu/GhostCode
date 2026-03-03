//! GhostCode 常驻守护进程核心模块
//!
//! 导出 Daemon 子模块供测试和外部使用
//! 实际的二进制入口在 main.rs
//!
//! @author Atlas.oi
//! @date 2026-03-01

pub mod lock;
pub mod paths;
pub mod process;
pub mod startup;
pub mod actor_mgmt;
pub mod dispatch;
pub mod group;
pub mod protocol;
pub mod server;
pub mod runner;
pub mod lifecycle;
pub mod messaging;
pub mod routing;
