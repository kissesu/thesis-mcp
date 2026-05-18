//! @file tables.rs
//! @description D.9.1 / D.9.2：表格 cell 段落缩进检查
//!
//! 职责：
//! 1. 遍历 `BodyChoice::WTbl` → `TableChoice2::WTr` → `TableRowChoice::WTc` → `TableCellChoice::WP`
//! 2. 对每个 cell 段落的 `pPr.ind` 检查缩进属性是否清零
//! 3. 支持嵌套表格（递归处理 `TableCellChoice::WTbl`）
//!
//! @author Atlas.oi
//! @date 2026-05-18

use ooxmlsdk::schemas::schemas_openxmlformats_org_wordprocessingml_2006_main::{
    BodyChoice, Paragraph, TableCellChoice, TableChoice2, TableRowChoice,
};
use thesis_types::{RuleId, Severity};

use crate::rules::Violation;

/// cell 段落缩进检查结果。
#[derive(Debug)]
pub struct CellIndentViolation {
    pub location: String,
    /// firstLineChars 非零
    pub first_line_chars: Option<i32>,
    /// leftChars 非零
    pub left_chars: Option<i32>,
    /// firstLine 非零（twips）
    pub first_line: Option<i32>,
    /// left 非零（twips）
    pub left: Option<i32>,
}

/// 从 body_choice 遍历所有表格 cell 段落，检查缩进属性。
///
/// 返回含有缩进非零的 `CellIndentViolation` 列表（D.9.1 / D.9.2 均包含）。
///
/// # 参数
/// - `body_choice`：文档 body 的顶层 choice 切片
#[must_use]
pub fn check_d91_cell_indent(body_choice: &[BodyChoice]) -> Vec<Violation> {
    let mut violations = Vec::new();
    walk_body(body_choice, &mut violations, "body");
    violations
}

/// 递归遍历 body_choice（支持嵌套表格）。
fn walk_body(choices: &[BodyChoice], violations: &mut Vec<Violation>, _prefix: &str) {
    for (tbl_idx, choice) in choices.iter().enumerate() {
        if let BodyChoice::WTbl(tbl_box) = choice {
            walk_table(&tbl_box.table_choice2, violations, tbl_idx);
        }
    }
}

/// 遍历表格的 TableChoice2 列表。
fn walk_table(table_choices: &[TableChoice2], violations: &mut Vec<Violation>, tbl_idx: usize) {
    for (tr_idx, tc2) in table_choices.iter().enumerate() {
        if let TableChoice2::WTr(tr_box) = tc2 {
            walk_row(&tr_box.table_row_choice, violations, tbl_idx, tr_idx);
        }
    }
}

/// 遍历 TableRow 的 TableRowChoice 列表。
fn walk_row(
    row_choices: &[TableRowChoice],
    violations: &mut Vec<Violation>,
    tbl_idx: usize,
    row_idx: usize,
) {
    for (cell_idx, rc) in row_choices.iter().enumerate() {
        if let TableRowChoice::WTc(tc_box) = rc {
            walk_cell(
                &tc_box.table_cell_choice,
                violations,
                tbl_idx,
                row_idx,
                cell_idx,
            );
        }
    }
}

/// 遍历 TableCell 的 TableCellChoice 列表（含嵌套表格递归）。
fn walk_cell(
    cell_choices: &[TableCellChoice],
    violations: &mut Vec<Violation>,
    tbl_idx: usize,
    row_idx: usize,
    cell_idx: usize,
) {
    for (p_idx, cc) in cell_choices.iter().enumerate() {
        match cc {
            TableCellChoice::WP(para_box) => {
                let loc = format!("tbl[{tbl_idx}]/tr[{row_idx}]/tc[{cell_idx}]/p[{p_idx}]");
                check_para_indent(para_box.as_ref(), &loc, violations);
            }
            TableCellChoice::WTbl(nested_tbl) => {
                // 嵌套表格：递归处理（nested tbl 的 tbl_idx 使用 p_idx 作代理索引）
                walk_table(&nested_tbl.table_choice2, violations, p_idx);
            }
            _ => {}
        }
    }
}

/// 检查单个段落的 pPr.ind 缩进属性。
///
/// D.9.1：firstLineChars 或 leftChars 非零 → Critical
/// D.9.2：firstLine 或 left 非零 → Critical
fn check_para_indent(para: &Paragraph, location: &str, violations: &mut Vec<Violation>) {
    let Some(pp) = para.paragraph_properties.as_ref() else {
        return;
    };
    let Some(ind) = pp.indentation.as_ref() else {
        return;
    };

    // D.9.1 检查：字符单位缩进
    let first_line_chars = ind.first_line_chars;
    let left_chars = ind.left_chars;

    if first_line_chars.is_some_and(|v| v != 0) {
        violations.push(Violation {
            rule_id: RuleId::D91,
            severity: Severity::Critical,
            location: location.to_owned(),
            actual: format!(
                "cell 段落 firstLineChars={} 非零（应为 0）",
                first_line_chars.unwrap_or(0)
            ),
        });
    }
    if left_chars.is_some_and(|v| v != 0) {
        violations.push(Violation {
            rule_id: RuleId::D91,
            severity: Severity::Critical,
            location: location.to_owned(),
            actual: format!(
                "cell 段落 leftChars={} 非零（应为 0）",
                left_chars.unwrap_or(0)
            ),
        });
    }

    // D.9.2 检查：磅值缩进（StringValue，需解析为整数）
    let first_line_str = ind.first_line.as_deref().unwrap_or("0");
    let left_str = ind.left.as_deref().unwrap_or("0");

    let first_line_val: i64 = first_line_str.parse().unwrap_or(0);
    let left_val: i64 = left_str.parse().unwrap_or(0);

    if first_line_val != 0 {
        violations.push(Violation {
            rule_id: RuleId::D92,
            severity: Severity::Critical,
            location: location.to_owned(),
            actual: format!("cell 段落 firstLine={first_line_val} 非零（应为 0）"),
        });
    }
    if left_val != 0 {
        violations.push(Violation {
            rule_id: RuleId::D92,
            severity: Severity::Critical,
            location: location.to_owned(),
            actual: format!("cell 段落 left={left_val} 非零（应为 0）"),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;

    /// 构建含表格的最小 docx fixture。
    fn build_docx_with_table(table_xml: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let document_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    {table_xml}
    <w:sectPr/>
  </w:body>
</w:document>"#
        );

        let rels_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"
    Target="word/document.xml"/>
</Relationships>"#;

        let word_rels_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
</Relationships>"#;

        let content_types_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml"
    ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;

        let buf = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(buf);
        let opts = SimpleFileOptions::default();

        zip.start_file("_rels/.rels", opts).unwrap();
        zip.write_all(rels_xml.as_bytes()).unwrap();
        zip.start_file("[Content_Types].xml", opts).unwrap();
        zip.write_all(content_types_xml.as_bytes()).unwrap();
        zip.start_file("word/document.xml", opts).unwrap();
        zip.write_all(document_xml.as_bytes()).unwrap();
        zip.start_file("word/_rels/document.xml.rels", opts)
            .unwrap();
        zip.write_all(word_rels_xml.as_bytes()).unwrap();

        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn test_d91_cell_indent_violation() {
        // 表格 cell 段落含 firstLineChars=200 → D.9.1 违规
        let table_xml = r#"<w:tbl xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:tr>
    <w:tc>
      <w:p>
        <w:pPr>
          <w:ind w:firstLineChars="200" w:firstLine="360"/>
        </w:pPr>
        <w:r><w:t>缩进过大的单元格</w:t></w:r>
      </w:p>
    </w:tc>
  </w:tr>
</w:tbl>"#;

        let docx_bytes = build_docx_with_table(table_xml);
        let doc = Document::load_bytes(&docx_bytes).expect("加载文档不应出错");

        // 通过 Document::load_bytes 加载后，需要从 body_choice 获取
        // 重新打开以访问 body_choice
        let violations = check_d91_from_bytes(&docx_bytes);

        let d91: Vec<_> = violations
            .iter()
            .filter(|v| v.rule_id == RuleId::D91)
            .collect();
        assert!(
            !d91.is_empty(),
            "firstLineChars 非零应触发 D.9.1，实际：{violations:?}"
        );
        let _ = doc;
    }

    #[test]
    fn test_d91_cell_indent_clean() {
        // 表格 cell 段落缩进全零 → 无违规
        let table_xml = r#"<w:tbl xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:tr>
    <w:tc>
      <w:p>
        <w:pPr>
          <w:ind w:firstLineChars="0" w:leftChars="0" w:firstLine="0" w:left="0"/>
        </w:pPr>
        <w:r><w:t>合规单元格</w:t></w:r>
      </w:p>
    </w:tc>
  </w:tr>
</w:tbl>"#;

        let docx_bytes = build_docx_with_table(table_xml);
        let violations = check_d91_from_bytes(&docx_bytes);
        assert!(
            violations.is_empty(),
            "零缩进 cell 不应有违规：{violations:?}"
        );
    }

    /// 辅助：从 docx 字节加载 body_choice 并运行 check_d91_cell_indent。
    fn check_d91_from_bytes(docx_bytes: &[u8]) -> Vec<Violation> {
        use ooxmlsdk::parts::wordprocessing_document::WordprocessingDocument;
        use std::io::Cursor;

        let mut package =
            WordprocessingDocument::new(Cursor::new(docx_bytes)).expect("打开 package 不应出错");
        let main_part = package.main_document_part().expect("main part 不应出错");
        let root = main_part.root_element(&mut package).expect("root 不应出错");
        let body = root.body.as_ref().expect("body 不应为 None");
        check_d91_cell_indent(&body.body_choice)
    }
}
