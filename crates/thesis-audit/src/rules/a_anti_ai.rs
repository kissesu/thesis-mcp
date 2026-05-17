//! @file rules/a_anti_ai.rs
//! @description A 系规则：反 AI 痕迹检测
//!
//! 规则覆盖：
//! - A.1：黑词列表（AI 惯用套话）
//! - A.5：em-dash（—）用于非破折号场景（暂 stub，待 L2.1b）
//! - A.6：CJK 间距异常（暂 stub）
//! - A.7：英文单词前后缺空格（暂 stub）
//! - A.9：括号风格混用（暂 stub）
//!
//! @author Atlas.oi
//! @date 2026-05-17

use thesis_types::{RuleId, Severity};

use crate::document::DocParagraph;
use crate::rules::Violation;

// ============================================
// A.1 黑词列表
// 来源：AI 写作中高频出现的套话词汇
// 实际投产时由配置文件覆盖，此处硬编码用于测试和初始投产
//
// 策略说明：
// 这里的黑词列表是 AI 偏好的口水话/八股 phrasing。
// 不包含常见学术连接词（如"然而"/"因此"/"另外"），它们出现频率高，
// 单独命中无意义。L2.1b 应接入"AI 偏好短语库 + N-gram 上下文"做真实判定。
// ============================================
// TODO(L2.1b): 实现可配置黑词列表覆盖路径
const BLACKWORDS: &[&str] = &[
    // 套话类
    "毋庸置疑",
    "不言而喻",
    "显而易见",
    "综上所述",
    "由此可见",
    "总而言之",
    "不难看出",
    "值得注意的是",
    "不容忽视",
    // AI 惯用连接词（仅保留明显口水话，去除"然而"等常见学术连接词）
    "并非偶然",
    "不仅如此",
    "与此同时",
    "在此基础上",
    "从这个角度来看",
    // 程度副词堆叠
    "极为重要",
    "至关重要",
    "举足轻重",
    "深远影响",
    "深刻影响",
    // AI 结尾套语
    "为未来研究提供了",
    "奠定了坚实基础",
    "具有重要的理论意义和实践价值",
    "进一步深入研究",
];

/// 检查段落列表中是否包含 A.1 黑词。
///
/// 每命中一个黑词产生一条 `Violation`，同一段落可产生多条。
#[must_use]
pub fn check_a1_blackwords(paragraphs: &[DocParagraph]) -> Vec<Violation> {
    let mut violations = Vec::new();

    for para in paragraphs {
        for &word in BLACKWORDS {
            if para.text.contains(word) {
                violations.push(Violation {
                    rule_id: RuleId::A1,
                    severity: Severity::Warning,
                    location: format!("body/p[{}]", para.index),
                    actual: format!("包含黑词：{word}"),
                });
            }
        }
    }

    violations
}

/// A.5：em-dash 滥用检测（stub，待 L2.1b 实现）
// stub: implement in L2.1b sub-task
#[allow(dead_code)]
#[must_use]
pub fn check_a5_em_dash(_paragraphs: &[DocParagraph]) -> Vec<Violation> {
    todo!("L2.1b: A.5 em-dash 检测 — 扫描 run 中的 U+2014 字符，判断是否非法使用")
}

/// A.6：CJK 与英文/数字间距检测（stub，待 L2.1b 实现）
// stub: implement in L2.1b sub-task
#[allow(dead_code)]
#[must_use]
pub fn check_a6_cjk_spacing(_paragraphs: &[DocParagraph]) -> Vec<Violation> {
    todo!("L2.1b: A.6 CJK 间距 — 检测中英文混排时缺少空格的情况")
}

/// A.7：英文单词前后空格检测（stub，待 L2.1b 实现）
// stub: implement in L2.1b sub-task
#[allow(dead_code)]
#[must_use]
pub fn check_a7_english_spacing(_paragraphs: &[DocParagraph]) -> Vec<Violation> {
    todo!("L2.1b: A.7 英文间距 — 检测英文单词与中文字符相邻但缺空格")
}

/// A.9：括号风格混用检测（stub，待 L2.1b 实现）
// stub: implement in L2.1b sub-task
#[allow(dead_code)]
#[must_use]
pub fn check_a9_bracket_style(_paragraphs: &[DocParagraph]) -> Vec<Violation> {
    todo!("L2.1b: A.9 括号风格 — 检测中英文括号混用（例如 （abc）vs (abc)）")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::DocParagraph;

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
    fn test_a1_blackword_detected() {
        // 含黑词「毋庸置疑」应命中一条 Violation
        let paras = vec![make_para(0, "毋庸置疑，本研究具有重要价值。")];
        let violations = check_a1_blackwords(&paras);
        assert!(!violations.is_empty(), "应检测到 A.1 黑词违规");
        assert_eq!(violations[0].rule_id, RuleId::A1);
        assert!(violations[0].actual.contains("毋庸置疑"));
        assert_eq!(violations[0].location, "body/p[0]");
    }

    #[test]
    fn test_a1_multiple_blackwords_in_one_para() {
        // 含两个黑词应产生两条 Violation
        let paras = vec![make_para(1, "综上所述，不难看出本文结论是正确的。")];
        let violations = check_a1_blackwords(&paras);
        // 「综上所述」和「不难看出」各一条
        assert_eq!(violations.len(), 2, "应产生 2 条黑词违规");
    }

    #[test]
    fn test_a1_clean_paragraph_passes() {
        // 正常学术表达不应命中
        let paras = vec![make_para(0, "本研究采用对比实验方法，对三组被试进行测量。")];
        let violations = check_a1_blackwords(&paras);
        assert!(
            violations.is_empty(),
            "正常段落不应有 A.1 违规，实际：{violations:?}"
        );
    }

    #[test]
    fn test_a1_empty_paragraphs_passes() {
        // 空文档不应崩溃
        let violations = check_a1_blackwords(&[]);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_a1_severity_is_warning() {
        // A.1 定级为 Warning（不阻断写入，但要记录）
        let paras = vec![make_para(0, "毋庸置疑")];
        let v = &check_a1_blackwords(&paras)[0];
        assert_eq!(v.severity, Severity::Warning);
    }
}
