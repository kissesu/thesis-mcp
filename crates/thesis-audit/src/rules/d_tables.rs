//! @file rules/d_tables.rs
//! @description D 系规则：表格 cell 段落属性清零检查（stub，待 L2.1b 实现）
//!
//! 规则覆盖：
//! - D.9.1：表格 cell 段落的 firstLineChars / leftChars 必须为 0
//! - D.9.2：表格 cell 段落的 firstLine / left 缩进必须为 0
//!
//! 完整实现需要 tables.rs 提供表格段落的结构化数据。
//!
//! @author Atlas.oi
//! @date 2026-05-17

// stub: implement in L2.1b sub-task
// D.9.x 检测依赖 tables.rs 对 w:tbl/w:tr/w:tc/w:p/w:pPr 的遍历提取。
// tables.rs 在 L2.1b 中实现后，此处填入检查逻辑。

use crate::document::DocParagraph;
use crate::rules::Violation;

/// D.9.1：表格 cell 段落 firstLineChars/leftChars 必须为 0（stub）
// stub: implement in L2.1b sub-task
#[allow(dead_code)]
#[must_use]
pub fn check_d91_cell_indent_chars(_paragraphs: &[DocParagraph]) -> Vec<Violation> {
    todo!("L2.1b: D.9.1 — 遍历表格 cell 段落的 pPr.ind，验证 firstLineChars=0 leftChars=0")
}

/// D.9.2：表格 cell 段落 firstLine/left 必须为 0（stub）
// stub: implement in L2.1b sub-task
#[allow(dead_code)]
#[must_use]
pub fn check_d92_cell_indent(_paragraphs: &[DocParagraph]) -> Vec<Violation> {
    todo!("L2.1b: D.9.2 — 遍历表格 cell 段落的 pPr.ind，验证 firstLine=0 left=0")
}
