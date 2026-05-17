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

use std::path::Path;

use thesis_types::{RuleId, Severity};
use tracing::warn;

use crate::document::DocParagraph;
use crate::rules::Violation;

/// 黑词配置文件名，相对于 `thesis_dir`。
const BLACKWORDS_FILENAME: &str = "blackwords.txt";

// ============================================
// A.1 黑词列表
// 来源：AI 写作中高频出现的套话词汇
// 实际投产时由 thesis_dir/blackwords.txt 覆盖，此处硬编码作为内置兜底
//
// 策略说明：
// 这里的黑词列表是 AI 偏好的口水话/八股 phrasing。
// 不包含常见学术连接词（如"然而"/"因此"/"另外"），它们出现频率高，
// 单独命中无意义。可配置路径见 `load_blackwords`。
// ============================================
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

/// 加载黑词列表，优先从 `thesis_dir/blackwords.txt` 读取，否则返回内置列表。
///
/// 解析规则：
/// - 每行一个词，去除首尾空白
/// - 以 `#` 开头的行为注释，跳过
/// - 空行跳过
///
/// 错误处理策略：
/// - 文件不存在（`NotFound`）：静默跳过，直接用内置列表（属于正常配置缺失场景）
/// - 文件存在但不可读（权限拒绝、坏 UTF-8 等）：`tracing::warn!` 记录错误，
///   降回内置列表——错误可见但不中断审计流程
///
/// # 参数
/// - `thesis_dir`：论文配置目录（通常为 docx 同级的 `.thesis/`），`None` 时直接返回内置列表
pub(crate) fn load_blackwords(thesis_dir: Option<&Path>) -> Vec<String> {
    // 只有提供了目录时才尝试加载
    if let Some(dir) = thesis_dir {
        let blackwords_path = dir.join(BLACKWORDS_FILENAME);
        match std::fs::read_to_string(&blackwords_path) {
            Ok(content) => {
                let words: Vec<String> = content
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty() && !line.starts_with('#'))
                    .map(str::to_owned)
                    .collect();
                // 文件存在但为空时仍返回内置列表，避免关闭所有检测
                if !words.is_empty() {
                    return words;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // 文件不存在：属于正常配置缺失，静默跳过
            }
            Err(e) => {
                // 文件存在但读取失败（权限拒绝、坏 UTF-8 等）：记录警告，不静默吞错
                warn!(
                    error = ?e,
                    path = %blackwords_path.display(),
                    "blackwords.txt 不可读，降回内置黑词列表"
                );
            }
        }
    }
    // 兜底：返回内置黑词列表
    BLACKWORDS.iter().map(ToString::to_string).collect()
}

/// 检查段落列表中是否包含 A.1 黑词。
///
/// 每命中一个黑词产生一条 `Violation`，同一段落可产生多条。
///
/// # 参数
/// - `paragraphs`：文档段落列表
/// - `blackwords`：黑词列表，由 `load_blackwords` 提供，支持运行时替换
#[must_use]
pub fn check_a1_blackwords(paragraphs: &[DocParagraph], blackwords: &[String]) -> Vec<Violation> {
    let mut violations = Vec::new();

    for para in paragraphs {
        for word in blackwords {
            if para.text.contains(word.as_str()) {
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

    /// 返回内置黑词列表（供单元测试注入，避免测试绑死到 BLACKWORDS 常量）
    fn builtin_blackwords() -> Vec<String> {
        load_blackwords(None)
    }

    #[test]
    fn test_a1_blackword_detected() {
        // 含黑词「毋庸置疑」应命中一条 Violation
        let paras = vec![make_para(0, "毋庸置疑，本研究具有重要价值。")];
        let violations = check_a1_blackwords(&paras, &builtin_blackwords());
        assert!(!violations.is_empty(), "应检测到 A.1 黑词违规");
        assert_eq!(violations[0].rule_id, RuleId::A1);
        assert!(violations[0].actual.contains("毋庸置疑"));
        assert_eq!(violations[0].location, "body/p[0]");
    }

    #[test]
    fn test_a1_multiple_blackwords_in_one_para() {
        // 含两个黑词应产生两条 Violation
        let paras = vec![make_para(1, "综上所述，不难看出本文结论是正确的。")];
        let violations = check_a1_blackwords(&paras, &builtin_blackwords());
        // 「综上所述」和「不难看出」各一条
        assert_eq!(violations.len(), 2, "应产生 2 条黑词违规");
    }

    #[test]
    fn test_a1_clean_paragraph_passes() {
        // 正常学术表达不应命中
        let paras = vec![make_para(0, "本研究采用对比实验方法，对三组被试进行测量。")];
        let violations = check_a1_blackwords(&paras, &builtin_blackwords());
        assert!(
            violations.is_empty(),
            "正常段落不应有 A.1 违规，实际：{violations:?}"
        );
    }

    #[test]
    fn test_a1_empty_paragraphs_passes() {
        // 空文档不应崩溃
        let violations = check_a1_blackwords(&[], &builtin_blackwords());
        assert!(violations.is_empty());
    }

    #[test]
    fn test_a1_severity_is_warning() {
        // A.1 定级为 Warning（不阻断写入，但要记录）
        let paras = vec![make_para(0, "毋庸置疑")];
        let v = &check_a1_blackwords(&paras, &builtin_blackwords())[0];
        assert_eq!(v.severity, Severity::Warning);
    }

    // ========================
    // load_blackwords 单元测试
    // ========================

    #[test]
    fn test_load_blackwords_thesis_dir_none_returns_default() {
        // None → 内置列表，长度与 BLACKWORDS 常量一致
        let words = load_blackwords(None);
        assert_eq!(
            words.len(),
            BLACKWORDS.len(),
            "None 时应返回内置黑词列表，长度期望 {}，实际 {}",
            BLACKWORDS.len(),
            words.len()
        );
    }

    #[test]
    fn test_load_blackwords_missing_file_returns_default() {
        // 目录存在但没有 blackwords.txt → 内置列表
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let words = load_blackwords(Some(tmp_dir.path()));
        assert_eq!(
            words.len(),
            BLACKWORDS.len(),
            "无配置文件时应返回内置黑词列表"
        );
    }

    #[test]
    fn test_load_blackwords_from_existing_file() {
        // TempDir 中写入 blackwords.txt（3 个词 + 1 注释行 + 1 空行）
        // 期望：只返回 3 个有效词
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let blackwords_path = tmp_dir.path().join("blackwords.txt");
        std::fs::write(
            &blackwords_path,
            "# 这是注释行\n自定义黑词甲\n自定义黑词乙\n\n自定义黑词丙\n",
        )
        .unwrap();

        let words = load_blackwords(Some(tmp_dir.path()));
        assert_eq!(words.len(), 3, "应解析出 3 个有效黑词，实际：{words:?}");
        assert!(words.contains(&"自定义黑词甲".to_owned()));
        assert!(words.contains(&"自定义黑词乙".to_owned()));
        assert!(words.contains(&"自定义黑词丙".to_owned()));
    }

    #[test]
    fn test_load_blackwords_invalid_utf8_falls_back_to_builtin() {
        // blackwords.txt 含无效 UTF-8 字节序列（0xFF 0xFE 是非法 UTF-8 前导字节）
        // 期望：read_to_string 失败 → tracing::warn! 发出（可在 audit log 中观察）→ 返回内置列表
        //
        // 注意：此处不断言 tracing 事件（需引入 tracing-test dev-dep 才能捕获），
        // 但 warn! 调用路径在 `load_blackwords` Err(_) 分支中，代码审查可见。
        // 若将来引入 tracing-test，可改为 #[traced_test] + assert!(logs_contain("不可读"))。
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let blackwords_path = tmp_dir.path().join(BLACKWORDS_FILENAME);
        // 写入非法 UTF-8：0xFF 0xFE 是 BOM 但不合法的独立 UTF-8 序列
        std::fs::write(&blackwords_path, b"\xff\xfe invalid utf8 \x80\x81").unwrap();

        let words = load_blackwords(Some(tmp_dir.path()));

        // 读取失败 → 降回内置列表，长度与 BLACKWORDS 常量一致
        assert_eq!(
            words.len(),
            BLACKWORDS.len(),
            "坏 UTF-8 文件应降回内置列表，实际返回 {} 词",
            words.len()
        );
        // 内置列表首条应包含已知黑词
        assert!(
            words.iter().any(|w| w == "毋庸置疑"),
            "内置列表应含「毋庸置疑」，实际：{words:?}"
        );
    }
}
