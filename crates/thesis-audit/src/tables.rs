//! @file tables.rs
//! @description 表格结构解析：提取 cell 段落及其 pPr
//!
//! 职责（待 L2.1b 实现）：
//! - 使用 quick-xml 流式扫描或 ooxmlsdk 类型化遍历 `w:tbl/w:tr/w:tc/w:p/w:pPr`
//! - 支持嵌套表格（递归处理 `w:tbl` 内的 `w:tbl`）
//! - 产出 `CellParagraph` 列表供 d_tables.rs 规则检查
//!
//! @author Atlas.oi
//! @date 2026-05-17

// stub: implement in L2.1b sub-task
// 遍历策略：使用 BodyChoice::WTbl 进入表格，递归处理嵌套表格

/// 表格内 cell 段落，携带位置信息和缩进属性。
#[allow(dead_code)]
pub struct CellParagraph {
    /// 位置描述，如 "tbl[0]/tr[1]/tc[2]/p[0]"
    pub location: String,
    /// firstLineChars 属性值（twips/50，0 表示无缩进）
    pub first_line_chars: Option<i32>,
    /// leftChars 属性值
    pub left_chars: Option<i32>,
    /// firstLine 缩进（twips）
    pub first_line: Option<i32>,
    /// left 缩进（twips）
    pub left: Option<i32>,
}

/// 从 body_choice 中提取所有表格 cell 段落（stub）
// stub: implement in L2.1b sub-task
#[allow(dead_code)]
#[must_use]
pub fn extract_cell_paragraphs(
    _body_choice: &[ooxmlsdk::schemas::schemas_openxmlformats_org_wordprocessingml_2006_main::BodyChoice],
) -> Vec<CellParagraph> {
    todo!("L2.1b: 遍历 BodyChoice::WTbl → TblRow → TblRowContent::WTc → cell paragraphs")
}
