//! @file comments.rs
//! @description 批注内容提取 + A.1 黑词扫描（P3，轻实现）
//!
//! 批注存储于 word/comments.xml（WordprocessingCommentsPart）。
//! 策略：quick-xml 流式扫描 comments.xml 提取 `<w:t>` 文本，运行 A.1 检测。
//!
//! @author Atlas.oi
//! @date 2026-05-18

use thesis_types::{RuleId, Severity};

use crate::rules::Violation;
use crate::xml_utils::{extract_paragraphs_from_xml, read_zip_entry};

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
