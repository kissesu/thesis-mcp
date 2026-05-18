//! @file rules/d_tables.rs
//! @description D 系规则：表格 cell 段落属性清零检查
//!
//! 规则覆盖：
//! - D.9.1：表格 cell 段落的 firstLineChars / leftChars 必须为 0
//! - D.9.2：表格 cell 段落的 firstLine / left 缩进必须为 0
//!
//! 实现：调用 `tables::check_d91_cell_indent`，将结果聚合。
//!
//! @author Atlas.oi
//! @date 2026-05-18

use ooxmlsdk::schemas::schemas_openxmlformats_org_wordprocessingml_2006_main::BodyChoice;

use crate::rules::Violation;
use crate::tables::check_d91_cell_indent;

/// D.9.1 / D.9.2：表格 cell 段落缩进检查入口。
///
/// 委托 `tables::check_d91_cell_indent` 执行实际检查，直接透传结果。
#[must_use]
pub fn check_d9x_all(body_choice: &[BodyChoice]) -> Vec<Violation> {
    check_d91_cell_indent(body_choice)
}
