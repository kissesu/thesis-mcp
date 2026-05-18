//! @file textbox.rs
//! @description HC-7：文本框内容提取 + A.1 黑词扫描
//!
//! 策略：ooxmlsdk 类型化模型不覆盖 `<w:drawing>/<mc:AlternateContent>/<wps:txbx>` 结构，
//! 改用 quick-xml 流式扫描 document.xml 字节，提取文本框段落文本。
//!
//! 扫描目标元素：
//! - `<mc:AlternateContent>` → `<mc:Choice>` → `<wps:txbx>` → `<w:p>/<w:r>/<w:t>`
//! - `<v:textbox>` → `<w:txbxContent>` → `<w:p>/<w:r>/<w:t>`
//!
//! @author Atlas.oi
//! @date 2026-05-18

use quick_xml::Reader;
use quick_xml::events::Event;
use thesis_types::{RuleId, Severity};

use crate::rules::Violation;

/// 从 docx zip 字节中提取 document.xml，流式扫描文本框内的 `<w:t>` 文本。
///
/// 返回每个文本框段落的 `(location, text)` 对，location 格式：`"textbox[N]/p[M]"`。
#[must_use]
pub fn extract_textbox_paragraphs(docx_bytes: &[u8]) -> Vec<(String, String)> {
    // ============================================
    // 第一步：从 zip 中读取 word/document.xml 字节
    // ============================================
    let Some(xml_bytes) = read_zip_entry(docx_bytes, "word/document.xml") else {
        return Vec::new();
    };

    scan_textbox_paragraphs(&xml_bytes)
}

/// 从 zip 字节中读取指定条目名称的内容。
fn read_zip_entry(zip_bytes: &[u8], entry_name: &str) -> Option<Vec<u8>> {
    use std::io::Read;
    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor).ok()?;
    let mut entry = archive.by_name(entry_name).ok()?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// quick-xml 流式扫描：追踪文本框上下文，提取 `<w:t>` 内容。
///
/// 状态机：
/// - `in_textbox`：是否在 `<wps:txbx>` 或 `<v:textbox>` 内
/// - `in_paragraph`：是否在文本框内的 `<w:p>` 内
/// - `in_t`：是否在 `<w:t>` 内
fn scan_textbox_paragraphs(xml_bytes: &[u8]) -> Vec<(String, String)> {
    let mut reader = Reader::from_reader(xml_bytes);
    reader.config_mut().trim_text(false);

    let mut results: Vec<(String, String)> = Vec::new();
    let mut buf = Vec::new();

    // 状态追踪
    let mut textbox_depth: i32 = 0; // 进入文本框容器的嵌套深度
    let mut textbox_count: usize = 0;
    let mut para_count: usize = 0;
    let mut in_t = false;
    let mut current_para_text = String::new();
    let mut current_para_loc = String::new();

    // 追踪全局元素深度，用于正确配对 start/end
    let mut depth: i32 = 0;
    let mut textbox_start_depth: i32 = 0;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                depth += 1;
                let name = e.name();
                let local = local_name(name.as_ref());

                // 检测进入文本框容器（wps:txbx 或 v:textbox 或 w:txbxContent）
                if local == b"txbx" || local == b"textbox" || local == b"txbxContent" {
                    if textbox_depth == 0 {
                        textbox_start_depth = depth;
                        textbox_count += 1;
                        para_count = 0;
                    }
                    textbox_depth += 1;
                }

                if textbox_depth > 0 {
                    if local == b"p" {
                        // 进入文本框内段落
                        current_para_text.clear();
                        current_para_loc =
                            format!("textbox[{}]/p[{}]", textbox_count - 1, para_count);
                    } else if local == b"t" {
                        in_t = true;
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());

                if textbox_depth > 0 {
                    if local == b"t" {
                        in_t = false;
                    } else if local == b"p" && textbox_depth > 0 {
                        // 段落结束，记录结果
                        if !current_para_text.is_empty() || !current_para_loc.is_empty() {
                            results.push((
                                std::mem::take(&mut current_para_loc),
                                std::mem::take(&mut current_para_text),
                            ));
                        }
                        para_count += 1;
                    }
                }

                // 检测退出文本框容器
                if local == b"txbx" || local == b"textbox" || local == b"txbxContent" {
                    if textbox_depth > 0 {
                        textbox_depth -= 1;
                    }
                    // 若退出顶层文本框，重置深度记录
                    if depth == textbox_start_depth {
                        textbox_start_depth = 0;
                    }
                }

                depth -= 1;
            }
            Ok(Event::Text(ref e)) if in_t && textbox_depth > 0 => {
                if let Ok(text) = e.decode() {
                    current_para_text.push_str(&text);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    results
}

/// 提取 XML 名称的本地部分（去掉命名空间前缀）。
fn local_name(name: &[u8]) -> &[u8] {
    if let Some(pos) = name.iter().rposition(|&b| b == b':') {
        &name[pos + 1..]
    } else {
        name
    }
}

/// 扫描文本框内容中的 A.1 黑词，返回 Violation 列表。
///
/// # 参数
/// - `docx_bytes`：docx zip 字节
/// - `blackwords`：黑词列表
#[must_use]
pub fn check_textbox_blackwords(docx_bytes: &[u8], blackwords: &[String]) -> Vec<Violation> {
    let paragraphs = extract_textbox_paragraphs(docx_bytes);
    let mut violations = Vec::new();

    for (location, text) in &paragraphs {
        for word in blackwords {
            if text.contains(word.as_str()) {
                violations.push(Violation {
                    rule_id: RuleId::A1,
                    severity: Severity::Warning,
                    location: location.clone(),
                    actual: format!("包含黑词：{word}"),
                });
            }
        }
    }

    violations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::a_anti_ai::load_blackwords;

    /// 构建含文本框的最小 docx fixture。
    /// 文本框使用 `<wps:txbx>` 内嵌于 `<w:drawing>` 的 WordprocessingShape 结构。
    fn build_docx_with_textbox(textbox_text: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        // 使用 v:textbox（VML 路径，更简单，被 quick-xml 扫描器同样覆盖）
        let document_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"
  xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
  xmlns:v="urn:schemas-microsoft-com:vml">
  <w:body>
    <w:p>
      <w:r>
        <w:pict>
          <v:textbox>
            <w:txbxContent>
              <w:p>
                <w:r><w:t>{textbox_text}</w:t></w:r>
              </w:p>
            </w:txbxContent>
          </v:textbox>
        </w:pict>
      </w:r>
    </w:p>
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
    fn test_textbox_detect_blackword() {
        // 文本框含黑词「毋庸置疑」→ 命中 A.1
        let docx_bytes = build_docx_with_textbox("毋庸置疑，本研究具有重要价值。");
        let blackwords = load_blackwords(None);
        let violations = check_textbox_blackwords(&docx_bytes, &blackwords);
        assert!(
            !violations.is_empty(),
            "文本框黑词应被检测，实际：{violations:?}"
        );
        assert_eq!(violations[0].rule_id, RuleId::A1);
        assert!(
            violations[0].location.starts_with("textbox["),
            "location 应以 textbox[ 开头，实际：{}",
            violations[0].location
        );
        assert!(violations[0].actual.contains("毋庸置疑"));
    }

    #[test]
    fn test_textbox_clean_no_violation() {
        // 文本框文本正常，不含黑词 → 无 violation
        let docx_bytes = build_docx_with_textbox("图 1 研究框架示意图");
        let blackwords = load_blackwords(None);
        let violations = check_textbox_blackwords(&docx_bytes, &blackwords);
        assert!(
            violations.is_empty(),
            "正常文本框不应有违规：{violations:?}"
        );
    }
}
