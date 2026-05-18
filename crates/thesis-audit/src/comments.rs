//! @file comments.rs
//! @description 批注内容提取 + A.1 黑词扫描（P3，轻实现）
//!
//! 批注存储于 word/comments.xml（WordprocessingCommentsPart）。
//! 策略：quick-xml 流式扫描 comments.xml 提取 `<w:t>` 文本，运行 A.1 检测。
//!
//! @author Atlas.oi
//! @date 2026-05-18

use quick_xml::Reader;
use quick_xml::events::Event;
use thesis_types::{RuleId, Severity};

use crate::rules::Violation;

/// 扫描 docx comments.xml 中的批注文本，运行 A.1 黑词检测。
///
/// 如果文档无 comments.xml，静默返回空 Vec。
#[must_use]
pub fn check_comments_blackwords(docx_bytes: &[u8], blackwords: &[String]) -> Vec<Violation> {
    let Some(xml_bytes) = read_zip_entry(docx_bytes, "word/comments.xml") else {
        return Vec::new();
    };

    let paragraphs = extract_paragraphs_from_xml(&xml_bytes, "comment");
    let mut violations = Vec::new();

    for (loc, text) in &paragraphs {
        for word in blackwords {
            if text.contains(word.as_str()) {
                violations.push(Violation {
                    rule_id: RuleId::A1,
                    severity: Severity::Warning,
                    location: loc.clone(),
                    actual: format!("包含黑词：{word}"),
                });
            }
        }
    }

    violations
}

/// 从 zip 字节读取指定条目。
fn read_zip_entry(zip_bytes: &[u8], entry_name: &str) -> Option<Vec<u8>> {
    use std::io::Read;
    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor).ok()?;
    let mut entry = archive.by_name(entry_name).ok()?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// quick-xml 流式扫描：提取所有 `<w:t>` 文本，按段落聚合。
fn extract_paragraphs_from_xml(xml_bytes: &[u8], prefix: &str) -> Vec<(String, String)> {
    let mut reader = Reader::from_reader(xml_bytes);
    reader.config_mut().trim_text(false);

    let mut results = Vec::new();
    let mut buf = Vec::new();

    let mut para_idx: usize = 0;
    let mut in_p = false;
    let mut in_t = false;
    let mut current_text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                if local == b"p" {
                    in_p = true;
                    current_text.clear();
                } else if local == b"t" && in_p {
                    in_t = true;
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                if local == b"t" {
                    in_t = false;
                } else if local == b"p" && in_p {
                    results.push((
                        format!("{prefix}[{para_idx}]"),
                        std::mem::take(&mut current_text),
                    ));
                    para_idx += 1;
                    in_p = false;
                }
            }
            Ok(Event::Text(ref e)) if in_t && in_p => {
                if let Ok(text) = e.decode() {
                    current_text.push_str(&text);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    results
}

fn local_name(name: &[u8]) -> &[u8] {
    if let Some(pos) = name.iter().rposition(|&b| b == b':') {
        &name[pos + 1..]
    } else {
        name
    }
}
