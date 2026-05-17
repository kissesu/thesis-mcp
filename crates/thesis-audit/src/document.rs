//! @file document.rs
//! @description 从 docx 文件加载并解析 OOXML 文档主体
//!
//! 职责：
//! 1. 从 `&Path` 打开 docx（ooxmlsdk `WordprocessingDocument`）
//! 2. 提取主文档 body 中所有段落（`Paragraph`）
//! 3. 将段落内所有 run 的文本拼合为字符串（供规则检查用）
//! 4. 暴露结构化的 `DocParagraph` 供规则层消费
//!
//! @author Atlas.oi
//! @date 2026-05-17

use std::path::Path;

use ooxmlsdk::parts::wordprocessing_document::WordprocessingDocument;
use ooxmlsdk::schemas::schemas_openxmlformats_org_wordprocessingml_2006_main::{
    BodyChoice, HyperlinkChoice, Paragraph, ParagraphChoice, RunChoice,
};

use crate::error::AuditError;

/// 段落在文档中的位置描述，用于 `Violation::location`。
///
/// 格式：`body/p[{index}]`，嵌套表格中的段落格式 `tbl[i]/tr[j]/tc[k]/p[l]`
/// 此处只处理 body 直接子段落；表格段落在 tables.rs 中处理。
#[derive(Debug, Clone)]
pub struct ParaLocation {
    /// 在 body.body_choice 中的段落序号（0-indexed）
    pub para_index: usize,
}

impl std::fmt::Display for ParaLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "body/p[{}]", self.para_index)
    }
}

/// 从 body 中提取的结构化段落。
///
/// 同时保存原始 `Paragraph` 引用和拼合文本，规则层可按需使用。
#[derive(Debug, Clone)]
pub struct DocParagraph {
    /// 在 body_choice 中对应的段落序号（0-indexed，只计 WP 段落）
    pub index: usize,
    /// 段落所有 run 文本拼合结果（不含删除文本 WDelText）
    pub text: String,
    /// 段落是否含有 numPr（即 word 自动编号属性）
    pub has_num_pr: bool,
    /// 如有 numPr，numId 的值
    pub num_id: Option<i32>,
    /// 如有 numPr，ilvl（编号层级，0-indexed）
    pub ilvl: Option<i64>,
    /// 段落样式 ID（如 "Heading1"、"1"）
    pub style_id: Option<String>,
}

/// 已加载的文档，持有段落列表。
///
/// 使用 `Document::load(path)` 构造。
pub struct Document {
    /// body 直接子段落（不含表格内段落）
    pub paragraphs: Vec<DocParagraph>,
}

impl Document {
    /// 从 docx 路径加载文档，提取所有 body 直接段落。
    ///
    /// 业务流程：
    /// 1. ooxmlsdk 打开 package
    /// 2. 取 main_document_part → root_element → body
    /// 3. 遍历 body_choice，匹配 BodyChoice::WP(paragraph)
    /// 4. 每个段落：拼合文本 + 提取 numPr
    pub fn load(path: &Path) -> Result<Self, AuditError> {
        // ============================================
        // 第一步：用 ooxmlsdk 打开 docx package
        // ============================================
        let mut package =
            WordprocessingDocument::new_from_file(path).map_err(AuditError::from_sdk)?;

        // ============================================
        // 第二步：取主文档 part 并加载根元素（Document）
        // ============================================
        // main_document_part() 返回 Result<MainDocumentPart, SdkError>（derive 生成）
        let main_part = package.main_document_part().map_err(AuditError::from_sdk)?;

        let root = main_part
            .root_element(&mut package)
            .map_err(AuditError::from_sdk)?;

        let body = root
            .body
            .as_ref()
            .ok_or_else(|| AuditError::SchemaViolation("Document 缺少 body".to_owned()))?;

        // ============================================
        // 第三步：遍历 body_choice 提取所有段落
        // ============================================
        let paragraphs = extract_paragraphs(body.body_choice.as_slice());

        Ok(Self { paragraphs })
    }
}

/// 从 `body_choice` 切片中提取所有直接段落，赋予连续索引。
fn extract_paragraphs(choices: &[BodyChoice]) -> Vec<DocParagraph> {
    let mut result = Vec::new();
    let mut para_index: usize = 0;

    for choice in choices {
        if let BodyChoice::WP(para_box) = choice {
            let para: &Paragraph = para_box.as_ref();
            let doc_para = build_doc_paragraph(para, para_index);
            result.push(doc_para);
            para_index += 1;
        }
    }

    result
}

/// 将单个 `Paragraph` 转换为 `DocParagraph`。
fn build_doc_paragraph(para: &Paragraph, index: usize) -> DocParagraph {
    // 提取 numPr
    let (has_num_pr, num_id, ilvl) = extract_num_pr(para);

    // 提取样式 ID
    // StringValue = String（简单类型别名），直接 clone
    let style_id = para
        .paragraph_properties
        .as_ref()
        .and_then(|pp| pp.paragraph_style_id.as_ref())
        .map(|ps| ps.val.clone());

    // 拼合文本
    let text = collect_paragraph_text(para);

    DocParagraph {
        index,
        text,
        has_num_pr,
        num_id,
        ilvl,
        style_id,
    }
}

/// 从段落中提取 numPr 信息（numId + ilvl）。
fn extract_num_pr(para: &Paragraph) -> (bool, Option<i32>, Option<i64>) {
    let Some(pp) = para.paragraph_properties.as_ref() else {
        return (false, None, None);
    };
    let Some(num_pr) = pp.numbering_properties.as_ref() else {
        return (false, None, None);
    };

    // Int32Value = i32（简单类型别名），直接使用
    let num_id = num_pr.numbering_id.as_ref().map(|n| n.val);

    let ilvl = num_pr
        .numbering_level_reference
        .as_ref()
        .map(|n| i64::from(n.val));

    (true, num_id, ilvl)
}

/// 拼合段落内所有 run 的文本（WT 变体的 xml_content）。
///
/// 跳过：删除文本（WDelText）、字段代码（WInstrText）、其他非文本内容。
fn collect_paragraph_text(para: &Paragraph) -> String {
    let mut buf = String::new();

    for choice in &para.paragraph_choice {
        match choice {
            ParagraphChoice::WR(run_box) => {
                // 普通 run：收集 WT 变体的文本
                for rc in &run_box.run_choice {
                    // 两层 if let 合并为 let-chain（Rust 2024）
                    if let RunChoice::WT(text_box) = rc
                        && let Some(content) = &text_box.xml_content
                    {
                        buf.push_str(content);
                    }
                }
            }
            ParagraphChoice::WHyperlink(hl_box) => {
                // 超链接内部也可能有 run
                for hl_choice in &hl_box.hyperlink_choice {
                    if let HyperlinkChoice::WR(run_box) = hl_choice {
                        for rc in &run_box.run_choice {
                            // 两层 if let 合并为一层（Rust 2024 let-chains）
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

/// 测试辅助：构建最小 docx zip 字节（供 document.rs 和 lib.rs 测试共用）
///
/// 此函数必须在 `#[cfg(test)]` 外声明（标记 `pub(crate)`），
/// 以便 lib.rs tests 模块通过 `crate::document::build_minimal_docx` 引用。
#[cfg(test)]
pub(crate) fn build_minimal_docx(body_xml: &str) -> Vec<u8> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let document_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:wpc="http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas"
  xmlns:cx="http://schemas.microsoft.com/office/drawing/2014/chartex"
  xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"
  xmlns:o="urn:schemas-microsoft-com:office:office"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
  xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
  xmlns:v="urn:schemas-microsoft-com:vml"
  xmlns:w10="urn:schemas-microsoft-com:office:word"
  xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml"
  mc:Ignorable="w14">
  <w:body>
    {body_xml}
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_single_paragraph() {
        let body_xml = r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:r><w:t>Hello World</w:t></w:r>
</w:p>"#;

        let docx_bytes = build_minimal_docx(body_xml);

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &docx_bytes).unwrap();

        let doc = Document::load(tmp.path()).unwrap();
        assert_eq!(doc.paragraphs.len(), 1);
        assert_eq!(doc.paragraphs[0].text, "Hello World");
    }

    #[test]
    fn test_extract_multiple_paragraphs() {
        let body_xml = r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:r><w:t>第一段</w:t></w:r>
</w:p>
<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:r><w:t>第二段</w:t></w:r>
</w:p>
<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:r><w:t>第三段</w:t></w:r>
</w:p>"#;

        let docx_bytes = build_minimal_docx(body_xml);
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &docx_bytes).unwrap();

        let doc = Document::load(tmp.path()).unwrap();
        assert_eq!(doc.paragraphs.len(), 3, "应提取 3 个段落");
        assert_eq!(doc.paragraphs[0].text, "第一段");
        assert_eq!(doc.paragraphs[2].text, "第三段");
    }
}
