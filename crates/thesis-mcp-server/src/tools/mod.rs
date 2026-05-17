//! @file tools/mod.rs
//! @description 工具模块聚合：re-export 各工具的入口函数和参数类型
//! @author Atlas.oi
//! @date 2026-05-17

pub mod audit;
pub mod init;

// Re-export 供 main.rs 直接引用
pub use audit::{AuditParams, StubAuditEngine, run_audit};
pub use init::{InitParams, run_init};
