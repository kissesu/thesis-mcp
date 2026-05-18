//! @file headers_footers.rs
//! @description HC-6：页眉页脚内容提取 + A.1 黑词扫描
//!
//! 职责：
//! 1. 通过 ooxmlsdk MainDocumentPart → header_parts / footer_parts 访问页眉页脚
//! 2. 遍历每个 Header/Footer 的 paragraphs，收集文本
//! 3. 对收集到的段落运行 A.1 黑词检测，位置标记为 "header[N]/p[M]" / "footer[N]/p[M]"
//!
//! 借用说明：
//! ooxmlsdk 的 `header_parts(&package)` 返回 `impl Iterator<Item = HeaderPart>` 并持有
//! `&package` 不可变借用，而 `root_element(&mut package)` 需要可变借用。
//! 策略：先将所有 part 收集为 `Vec`（消耗迭代器，释放 `&package` 借用），
//! 再逐一调用 `root_element(&mut package)`。
//!
//! @author Atlas.oi
//! @date 2026-05-18

use std::io::Cursor;

use ooxmlsdk::parts::footer_part::FooterPart;
use ooxmlsdk::parts::header_part::HeaderPart;
use ooxmlsdk::parts::wordprocessing_document::WordprocessingDocument;
use ooxmlsdk::schemas::schemas_openxmlformats_org_wordprocessingml_2006_main::{
    FooterChoice, HeaderChoice, HyperlinkChoice, ParagraphChoice, RunChoice,
};
use thesis_types::{RuleId, Severity};

use crate::rules::Violation;

/// 从 docx 字节中扫描所有页眉页脚，对其中段落运行 A.1 黑词检测。
///
/// 参数：
/// - `docx_bytes`：docx 文件字节（与 audit_full 共用同一份字节，不重读文件）
/// - `blackwords`：黑词列表（由 a_anti_ai::load_blackwords 提供）
#[must_use]
pub fn check_headers_footers_blackwords(
    docx_bytes: &[u8],
    blackwords: &[String],
) -> Vec<Violation> {
    let Ok(mut package) = WordprocessingDocument::new(Cursor::new(docx_bytes)) else {
        return Vec::new();
    };

    let Ok(main_part) = package.main_document_part() else {
        return Vec::new();
    };

    // ============================================
    // 第一步：先收集所有 HeaderPart 和 FooterPart（消耗迭代器，释放 &package 借用）
    // header_parts(&package) 持有 &package 不可变借用；
    // 收集进 Vec 后迭代器生命周期结束，&package 借用释放，
    // 之后可以调用 root_element(&mut package)
    // ============================================
    let header_parts: Vec<HeaderPart> = main_part.header_parts(&package).collect();
    let footer_parts: Vec<FooterPart> = main_part.footer_parts(&package).collect();

    let mut violations = Vec::new();

    // ============================================
    // 第二步：扫描所有 HeaderPart
    // ============================================
    for (hdr_idx, hdr_part) in header_parts.iter().enumerate() {
        let Ok(root) = hdr_part.root_element(&mut package) else {
            continue;
        };

        for (para_idx, choice) in root.header_choice.iter().enumerate() {
            if let HeaderChoice::WP(para_box) = choice {
                let text = collect_para_text_from_choices(&para_box.paragraph_choice);
                for word in blackwords {
                    if text.contains(word.as_str()) {
                        violations.push(Violation {
                            rule_id: RuleId::A1,
                            severity: Severity::Warning,
                            location: format!("header[{hdr_idx}]/p[{para_idx}]"),
                            actual: format!("包含黑词：{word}"),
                        });
                    }
                }
            }
        }
    }

    // ============================================
    // 第三步：扫描所有 FooterPart
    // ============================================
    for (ftr_idx, ftr_part) in footer_parts.iter().enumerate() {
        let Ok(root) = ftr_part.root_element(&mut package) else {
            continue;
        };

        for (para_idx, choice) in root.footer_choice.iter().enumerate() {
            if let FooterChoice::WP(para_box) = choice {
                let text = collect_para_text_from_choices(&para_box.paragraph_choice);
                for word in blackwords {
                    if text.contains(word.as_str()) {
                        violations.push(Violation {
                            rule_id: RuleId::A1,
                            severity: Severity::Warning,
                            location: format!("footer[{ftr_idx}]/p[{para_idx}]"),
                            actual: format!("包含黑词：{word}"),
                        });
                    }
                }
            }
        }
    }

    violations
}

/// 从段落 choice 列表中拼合文本（复用 document.rs 相同逻辑）
fn collect_para_text_from_choices(choices: &[ParagraphChoice]) -> String {
    let mut buf = String::new();
    for choice in choices {
        match choice {
            ParagraphChoice::WR(run_box) => {
                for rc in &run_box.run_choice {
                    if let RunChoice::WT(text_box) = rc
                        && let Some(content) = &text_box.xml_content
                    {
                        buf.push_str(content);
                    }
                }
            }
            ParagraphChoice::WHyperlink(hl_box) => {
                for hl_choice in &hl_box.hyperlink_choice {
                    if let HyperlinkChoice::WR(run_box) = hl_choice {
                        for rc in &run_box.run_choice {
                            if let RunChoice::WT(text_box) = rc
                                && let Some(content) = &text_box.xml_content
                            {
                                buf.push_str(content);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::a_anti_ai::load_blackwords;

    /// 构建含页脚的最小 docx fixture。
    pub(crate) fn build_docx_with_footer(footer_body_xml: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <w:body>
    <w:p><w:r><w:t>正文内容</w:t></w:r></w:p>
    <w:sectPr>
      <w:footerReference w:type="default" r:id="rId2"/>
    </w:sectPr>
  </w:body>
</w:document>"#;

        let footer_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  {footer_body_xml}
</w:ftr>"#
        );

        let rels_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"
    Target="word/document.xml"/>
</Relationships>"#;

        let word_rels_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId2"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer"
    Target="footer1.xml"/>
</Relationships>"#;

        let content_types_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml"
    ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/footer1.xml"
    ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>
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

        zip.start_file("word/footer1.xml", opts).unwrap();
        zip.write_all(footer_xml.as_bytes()).unwrap();

        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn test_headers_footers_detect_blackword_in_footer() {
        let footer_xml = r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:r><w:t>毋庸置疑，本研究具有重要价值。</w:t></w:r>
</w:p>"#;
        let docx_bytes = build_docx_with_footer(footer_xml);
        let blackwords = load_blackwords(None);

        let violations = check_headers_footers_blackwords(&docx_bytes, &blackwords);

        assert!(
            !violations.is_empty(),
            "页脚黑词应被检测，实际 violations: {violations:?}"
        );
        assert_eq!(violations[0].rule_id, RuleId::A1);
        assert!(
            violations[0].location.starts_with("footer["),
            "location 应以 footer[ 开头，实际：{}",
            violations[0].location
        );
        assert!(violations[0].actual.contains("毋庸置疑"));
    }

    #[test]
    fn test_headers_footers_clean_footer_no_violation() {
        let footer_xml = r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:r><w:t>第 1 页 共 N 页</w:t></w:r>
</w:p>"#;
        let docx_bytes = build_docx_with_footer(footer_xml);
        let blackwords = load_blackwords(None);
        let violations = check_headers_footers_blackwords(&docx_bytes, &blackwords);
        assert!(violations.is_empty(), "正常页脚不应有违规：{violations:?}");
    }
}
