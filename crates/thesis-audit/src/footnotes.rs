//! @file footnotes.rs
//! @description 脚注/尾注内容提取 + A.1 黑词扫描（P3，轻实现）
//!
//! 脚注存储于 word/footnotes.xml，尾注于 word/endnotes.xml。
//! 策略：quick-xml 流式扫描两个 part，对提取的 `<w:t>` 文本运行 A.1 检测。
//!
//! @author Atlas.oi
//! @date 2026-05-18

use thesis_types::{RuleId, Severity};

use crate::rules::Violation;
use crate::xml_utils::{extract_paragraphs_from_xml, read_zip_entry};

/// 扫描 docx footnotes.xml + endnotes.xml 中的文本，运行 A.1 黑词检测。
///
/// 两个 part 均不存在时静默返回空 Vec。
#[must_use]
pub fn check_footnotes_blackwords(docx_bytes: &[u8], blackwords: &[String]) -> Vec<Violation> {
    let mut violations = Vec::new();

    for (part_name, prefix) in &[
        ("word/footnotes.xml", "footnote"),
        ("word/endnotes.xml", "endnote"),
    ] {
        let Some(xml_bytes) = read_zip_entry(docx_bytes, part_name) else {
            continue;
        };

        let paragraphs = extract_paragraphs_from_xml(&xml_bytes, prefix);
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
    }

    violations
}
