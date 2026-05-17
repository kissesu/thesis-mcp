//! @file rules/e_format.rs
//! @description E 系规则：自动编号格式检查
//!
//! 规则覆盖：
//! - E.5.7：章节标题必须使用 Word 自动编号（numPr），不能手动键入数字
//! - E.5.8：参考文献条目必须使用自动编号（[%1] 格式）（部分实现，完整依赖 numbering.rs）
//!
//! 检测策略：
//! - 利用 `DocParagraph.style_id` 判断段落是否为标题样式（Heading1/2/3 或数字 1/2/3）
//! - 利用 `DocParagraph.has_num_pr` 判断是否使用了自动编号
//! - 如果是标题样式但无 numPr → E.5.7 违规
//! - 如果段落文本匹配手动章节号模式（如 "1. " 开头）但无 numPr → E.5.7 违规
//!
//! @author Atlas.oi
//! @date 2026-05-17

use thesis_types::{RuleId, Severity};

use crate::document::DocParagraph;
use crate::rules::Violation;

/// 判断样式 ID 是否为标题样式。
///
/// Word 内置标题样式有两种命名方式：
/// - 英文名：Heading1, Heading2, Heading3（中文 Word 也使用这些内部 ID）
/// - 数字简写：1, 2, 3（部分模板）
/// - 带连字符：heading-1 等（少数情况）
fn is_heading_style(style_id: &str) -> bool {
    matches!(
        style_id,
        "Heading1"
            | "Heading2"
            | "Heading3"
            | "Heading4"
            | "Heading5"
            | "heading1"
            | "heading2"
            | "heading3"
            | "1"
            | "2"
            | "3"
            | "4"
            | "5"
    )
}

/// 判断段落文本是否看起来像手动键入的章节号。
///
/// 匹配模式：
/// - `"1. 引言"` / `"2. 相关工作"` — 一级章节手动编号
/// - `"1.1 研究背景"` / `"2.3 实验设计"` — 二级章节
/// - `"1.1.1 ..."` — 三级章节
///
/// 注意：此启发式检测可能有误报（如正文中以数字开头的列表），
/// 但配合样式检测可降低误报率。
fn looks_like_manual_chapter_number(text: &str) -> bool {
    // 匹配 "数字. " 或 "数字.数字 " 或 "数字.数字.数字 " 开头
    let trimmed = text.trim_start();

    // 取前 20 个字符做快速扫描，避免长文本性能问题
    let prefix: String = trimmed.chars().take(20).collect();

    // 简单状态机：匹配 [0-9]+ ('.' [0-9]+)* (' '|'\t'|'　')
    let mut chars = prefix.chars().peekable();

    // 必须以数字开头
    if !chars.peek().is_some_and(char::is_ascii_digit) {
        return false;
    }

    // 消费第一组数字
    while chars.peek().is_some_and(char::is_ascii_digit) {
        chars.next();
    }

    // 接着必须有 '.' 或 '。'
    let Some('.') = chars.peek().copied() else {
        return false;
    };
    chars.next();

    // 后面可以是空格（代表 "1. 引言"）或继续数字（"1.1 ..."）
    match chars.peek().copied() {
        Some(' ' | '\t' | '　') | None => true,
        Some(c) if c.is_ascii_digit() => true,
        _ => false,
    }
}

/// E.5.7：检查章节标题是否使用了 Word 自动编号。
///
/// 命中条件（满足其一）：
/// 1. 段落样式 ID 是标题样式 且 无 numPr
/// 2. 段落文本以手动章节号模式开头（不依赖样式）且 无 numPr
///
/// 策略 2 用于捕获"用了 Heading 样式但手动输入了数字"这种情况，
/// 以及"没用 Heading 样式但文本像章节号"的情况。
#[must_use]
pub fn check_e57_chapter_numbering(paragraphs: &[DocParagraph]) -> Vec<Violation> {
    let mut violations = Vec::new();

    for para in paragraphs {
        let is_heading = para.style_id.as_deref().is_some_and(is_heading_style);

        let manual_prefix = looks_like_manual_chapter_number(&para.text);

        if (is_heading || manual_prefix) && !para.has_num_pr {
            // 确定命中原因描述
            let reason = if is_heading {
                format!(
                    "标题样式段落（{}）无 numPr 自动编号",
                    para.style_id.as_deref().unwrap_or("unknown")
                )
            } else {
                format!(
                    "段落文本以手动章节号开头（前 20 字符：{:.20}）但无 numPr",
                    para.text
                )
            };

            violations.push(Violation {
                rule_id: RuleId::E57,
                severity: Severity::Critical,
                location: format!("body/p[{}]", para.index),
                actual: reason,
            });
        }
    }

    violations
}

/// E.5.8：参考文献条目自动编号检查（部分实现）。
///
/// 完整实现依赖 `numbering.rs` 中对 `NumberingPart` 的解析。
/// 此处检测：段落文本是否以 "[数字]" 开头（手动编号特征）但无 numPr。
///
/// 完整 E.5.8 需要：验证 numPr 对应的 abstractNum 的 lvlText 是否为 "[%1]"，
/// 这部分逻辑在 L2.1b 中通过 numbering.rs 实现。
// stub: 完整实现在 L2.1b sub-task 中完成（需要 numbering.rs 提供 lvlText 查询）
#[must_use]
pub fn check_e58_reference_numbering(paragraphs: &[DocParagraph]) -> Vec<Violation> {
    let mut violations = Vec::new();

    for para in paragraphs {
        // 检测文本以 "[数字]" 开头（手动编号模式）
        if looks_like_manual_ref_number(&para.text) && !para.has_num_pr {
            violations.push(Violation {
                rule_id: RuleId::E58,
                severity: Severity::Critical,
                location: format!("body/p[{}]", para.index),
                actual: format!(
                    "参考文献段落以手动编号 [N] 开头但无 numPr：{:.30}",
                    para.text
                ),
            });
        }
    }

    violations
}

/// 判断段落文本是否以手动参考文献编号 `[N]` 开头。
fn looks_like_manual_ref_number(text: &str) -> bool {
    let trimmed = text.trim_start();
    if !trimmed.starts_with('[') {
        return false;
    }
    // 找匹配的 ']'
    let end = trimmed.find(']');
    let Some(end) = end else { return false };
    // 检查括号内是否全为数字
    let inner = &trimmed[1..end];
    !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::DocParagraph;

    fn make_para(
        index: usize,
        text: &str,
        has_num_pr: bool,
        style_id: Option<&str>,
    ) -> DocParagraph {
        DocParagraph {
            index,
            text: text.to_owned(),
            has_num_pr,
            num_id: if has_num_pr { Some(1) } else { None },
            ilvl: if has_num_pr { Some(0) } else { None },
            style_id: style_id.map(str::to_owned),
        }
    }

    // ========================
    // E.5.7 测试
    // ========================

    #[test]
    fn test_e57_heading_without_num_pr_is_violation() {
        // Heading1 样式但无 numPr → 违规
        let paras = vec![make_para(0, "引言", false, Some("Heading1"))];
        let v = check_e57_chapter_numbering(&paras);
        assert!(!v.is_empty(), "Heading1 无 numPr 应违规");
        assert_eq!(v[0].rule_id, RuleId::E57);
        assert_eq!(v[0].severity, Severity::Critical);
    }

    #[test]
    fn test_e57_manual_prefix_without_num_pr_is_violation() {
        // 手动键入 "1. 引言" 且无 numPr → 违规
        let paras = vec![make_para(2, "1. 引言", false, None)];
        let v = check_e57_chapter_numbering(&paras);
        assert!(!v.is_empty(), "手动章节号段落无 numPr 应违规");
        assert_eq!(v[0].location, "body/p[2]");
    }

    #[test]
    fn test_e57_clean_heading_with_num_pr_passes() {
        // Heading1 + 有 numPr → 通过
        let paras = vec![make_para(0, "引言", true, Some("Heading1"))];
        let v = check_e57_chapter_numbering(&paras);
        assert!(v.is_empty(), "有 numPr 的标题不应违规");
    }

    #[test]
    fn test_e57_normal_paragraph_passes() {
        // 普通段落（非标题、非手动编号前缀）→ 通过
        let paras = vec![make_para(
            0,
            "本研究采用实验对比方法，验证了假设 H1 的正确性。",
            false,
            None,
        )];
        let v = check_e57_chapter_numbering(&paras);
        assert!(v.is_empty(), "普通正文段落不应触发 E.5.7");
    }

    #[test]
    fn test_e57_two_level_manual_number_detected() {
        // "2.3 实验设计" 手动二级编号 → 违规
        let paras = vec![make_para(5, "2.3 实验设计", false, None)];
        let v = check_e57_chapter_numbering(&paras);
        assert!(!v.is_empty(), "二级手动章节号应触发 E.5.7");
    }

    // ========================
    // E.5.8 测试
    // ========================

    #[test]
    fn test_e58_manual_ref_number_detected() {
        // "[1] 作者, 标题..." 手动编号 → 违规
        let paras = vec![make_para(0, "[1] Smith J. Research. 2023.", false, None)];
        let v = check_e58_reference_numbering(&paras);
        assert!(!v.is_empty(), "手动参考文献编号应触发 E.5.8");
        assert_eq!(v[0].rule_id, RuleId::E58);
    }

    #[test]
    fn test_e58_auto_ref_passes() {
        // 有 numPr 的参考文献段落 → 通过
        let paras = vec![make_para(0, "[1] Smith J. Research. 2023.", true, None)];
        let v = check_e58_reference_numbering(&paras);
        assert!(v.is_empty(), "有 numPr 的参考文献不应违规");
    }

    // ========================
    // 辅助函数测试
    // ========================

    #[test]
    fn test_looks_like_manual_chapter_number() {
        assert!(looks_like_manual_chapter_number("1. 引言"));
        assert!(looks_like_manual_chapter_number("2.3 实验"));
        assert!(looks_like_manual_chapter_number("1.1.1 子节"));
        assert!(!looks_like_manual_chapter_number("本研究"));
        assert!(!looks_like_manual_chapter_number("图1 示意图"));
        assert!(!looks_like_manual_chapter_number(""));
    }
}
