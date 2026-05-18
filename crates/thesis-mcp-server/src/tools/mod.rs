//! @file tools/mod.rs
//! @description 工具模块聚合：re-export 各工具的入口函数和参数类型
//! @author Atlas.oi
//! @date 2026-05-17

pub mod audit;
pub mod audit_format;
pub mod init;
pub mod revise;
pub mod write_section;

// Re-export 供 main.rs 直接引用
// StubAuditEngine 仅用于 audit.rs 内部测试，不对外 re-export
pub use audit::{AuditParams, RealAuditEngine, run_audit};
pub use init::{InitParams, run_init};
pub use revise::{ReviseParams, run_revise};
pub use write_section::{WriteSectionParams, run_write_section};
