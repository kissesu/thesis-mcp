//! @file rules/c_citation.rs
//! @description C 系规则：引用标注格式检查（P3，轻实现）
//!
//! 规则覆盖：
//! - C.1：引用标注 `[N]` 必须以上标形式出现
//! - C.2：参考文献引用编号必须按首次出现顺序递增（首次引用 [1], [2], ..）
//!
//! 当前实现：基于文本层面的启发式检测。
//! - C.1：检测段落文本中是否含 `[N]`；完整 C.1 需要 run 级别的 vertAlign=superscript 检查
//!   （留给后续 L5 实现，当前返回空 Vec 避免误报）
//! - C.2：扫描所有段落文本，提取 `[N]` 数字序列，验证递增
//!
//! @author Atlas.oi
//! @date 2026-05-18

use thesis_types::{RuleId, Severity};

use crate::document::DocParagraph;
use crate::rules::Violation;

/// C.1：引用标注必须以上标形式出现。
///
/// 完整实现需要 Run.run_properties.vertical_text_alignment == superscript。
/// 当前：轻实现返回空 Vec（避免误报；完整 C.1 在 L5 run-level scan 中实现）。
#[must_use]
pub fn check_c1_citation_superscript(_paragraphs: &[DocParagraph]) -> Vec<Violation> {
    // C.1 完整实现需要遍历 run 级别的 RunProperties.verticalTextAlignment。
    // 当前阶段不做 run 级扫描，返回空（保守策略：宁漏报不误报）。
    Vec::new()
}

/// C.2：参考文献引用编号按顺序递增检查。
///
/// 扫描所有段落文本，提取 `[N]` 模式的数字，验证首次出现序列是否严格递增（从 1 开始）。
///
/// 注意：只验证出现过的编号之间的顺序，不要求连续（允许 [1] [3] 跳号）。
#[must_use]
pub fn check_c2_citation_order(paragraphs: &[DocParagraph]) -> Vec<Violation> {
    // 收集所有引用编号（按段落顺序，保留首次出现）
    let mut seen_nums: Vec<u32> = Vec::new();
    let mut violations = Vec::new();

    for para in paragraphs {
        let refs = extract_ref_numbers(&para.text);
        for num in refs {
            if !seen_nums.contains(&num) {
                // 首次出现此编号：检查是否 >= 当前最大值 + 1（允许跳号，但不允许乱序）
                if let Some(&last) = seen_nums.last()
                    && num <= last
                {
                    // 乱序：新编号比已见最大值还小
                    violations.push(Violation {
                        rule_id: RuleId::C2,
                        severity: Severity::Warning,
                        location: format!("body/p[{}]", para.index),
                        actual: format!("引用编号乱序：[{num}] 出现在 [{last}] 之后（应递增）"),
                    });
                }
                seen_nums.push(num);
            }
        }
    }

    violations
}

/// 从文本中提取所有 `[数字]` 模式的数字列表（按出现顺序）。
fn extract_ref_numbers(text: &str) -> Vec<u32> {
    let mut result = Vec::new();
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '[' {
            // 尝试读取数字序列
            let mut num_str = String::new();
            while chars.peek().is_some_and(|&c| c.is_ascii_digit()) {
                num_str.push(chars.next().unwrap());
            }
            // 检查结束字符是 ']'
            if chars.peek() == Some(&']') {
                chars.next();
                if let Ok(n) = num_str.parse::<u32>() {
                    result.push(n);
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_para(index: usize, text: &str) -> DocParagraph {
        DocParagraph {
            index,
            text: text.to_owned(),
            has_num_pr: false,
            num_id: None,
            ilvl: None,
            style_id: None,
        }
    }

    #[test]
    fn test_c2_in_order_passes() {
        let paras = vec![
            make_para(0, "本研究参考了[1]和[2]的方法。"),
            make_para(1, "后续采用[3]的框架。"),
        ];
        let v = check_c2_citation_order(&paras);
        assert!(v.is_empty(), "顺序引用不应有违规：{v:?}");
    }

    #[test]
    fn test_c2_out_of_order_detected() {
        // [2] 先出现，[1] 后出现 → C.2 违规
        let paras = vec![
            make_para(0, "本研究参考了[2]的方法。"),
            make_para(1, "也参考了[1]的框架。"),
        ];
        let v = check_c2_citation_order(&paras);
        assert!(!v.is_empty(), "乱序引用应触发 C.2：{v:?}");
        assert_eq!(v[0].rule_id, RuleId::C2);
    }

    #[test]
    fn test_c2_skip_allowed() {
        // [1] [3]（跳过 [2]）→ 顺序合规
        let paras = vec![make_para(0, "参考[1]。"), make_para(1, "参考[3]。")];
        let v = check_c2_citation_order(&paras);
        assert!(v.is_empty(), "允许跳号，不应违规：{v:?}");
    }

    #[test]
    fn test_extract_ref_numbers() {
        assert_eq!(extract_ref_numbers("本研究[1]参考[23]的方法"), vec![1, 23]);
        assert_eq!(extract_ref_numbers("无引用"), vec![]);
        assert_eq!(extract_ref_numbers("[abc]"), vec![]);
    }
}
