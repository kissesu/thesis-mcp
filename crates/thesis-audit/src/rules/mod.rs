//! @file rules/mod.rs
//! @description 规则子模块声明 + 通用 Violation 类型
//!
//! `Violation` 是所有规则函数的返回单元，由 `lib.rs` 汇总转换为 `CheckRow`。
//!
//! @author Atlas.oi
//! @date 2026-05-17

pub mod a_anti_ai;
pub mod c_citation;
pub mod d_tables;
pub mod e_format;

use thesis_types::{RuleId, Severity};

/// 单条规则命中记录。
///
/// 每个规则函数返回 `Vec<Violation>`；无命中则返回空 Vec。
/// `lib.rs` 将 `Vec<Violation>` 汇总为 `CheckRow`（一条规则一行）。
#[derive(Debug, Clone)]
pub struct Violation {
    /// 对应规则 ID
    pub rule_id: RuleId,
    /// 严重度
    pub severity: Severity,
    /// 命中位置描述（如 "body/p[3]"）
    pub location: String,
    /// 实际发现的内容摘要（如 "包含黑词：毋庸置疑"）
    pub actual: String,
}
