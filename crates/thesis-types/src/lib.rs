//! @file lib.rs
//! @description thesis-mcp 全工程共享类型：规则 ID / 严重度 / 写入操作 / Manifest / AuditResult
//! @author Atlas.oi
//! @date 2026-05-17

pub mod audit;
pub mod manifest;
pub mod rule;
pub mod severity;

// 重新导出常用类型，外部 crate 可直接 use thesis_types::RuleId
pub use audit::{AuditResult, CheckRow};
pub use manifest::{Manifest, WriteOp};
pub use rule::RuleId;
pub use severity::Severity;
