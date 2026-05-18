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
//! - 如果有 numPr 且提供了编号映射，则进一步验证 lvlText 格式是否符合规范
//!
//! @author Atlas.oi
//! @date 2026-05-18

use std::path::Path;

use thesis_types::{RuleId, Severity};
use tracing::warn;

use crate::document::DocParagraph;
use crate::numbering::NumIdLvlTexts;
use crate::rules::Violation;

/// 非编号章节白名单文件名，相对于 `thesis_dir`。
const EXEMPT_HEADINGS_FILENAME: &str = "non_numbered_headings.txt";

// ============================================
// 非编号章节内置白名单
//
// 按 GB/T 7714 与中国高校论文常规规范：
// 前置部分（封面、声明、摘要、目录）+ 后置部分（结论、参考文献、致谢、附录、索引）
// 用 Heading 样式但不参与正文章节自动编号。E.5.7 命中时这些段落应豁免。
//
// 匹配策略：
// - normalize_heading_text 去掉所有空白字符（含全角空格 U+3000）+ ASCII 小写化
// - 段落文本去白后以白名单任一条目（同样去白）开头 → 视为豁免
// - 仅在段落同时是 Heading 样式时生效（避免正文段落以"摘要"开头被误豁免）
//
// 用户可通过 `.thesis/non_numbered_headings.txt` 提供自定义列表覆盖（每行一个）。
// ============================================
const EXEMPT_HEADING_NAMES: &[&str] = &[
    // 中文 — 前置部分
    "摘要",
    "中文摘要",
    "英文摘要",
    "外文摘要",
    "关键词",
    "关键字",
    "目录",
    "图目录",
    "表目录",
    "插图目录",
    // 中文 — 后置部分
    "结论",
    "结论与展望",
    "总结",
    "总结与展望",
    "参考文献",
    "致谢",
    "附录",
    "索引",
    "缩略语",
    "缩略语表",
    "缩略词表",
    "符号说明",
    "符号表",
    "名词术语表",
    "攻读硕士学位期间发表的学术论文",
    "攻读博士学位期间发表的学术论文",
    "攻读学位期间发表的学术论文",
    "攻读学位期间的科研成果",
    "攻读学位期间发表论文情况",
    "个人简历",
    "作者简介",
    "原创性声明",
    "独创性声明",
    "学位论文版权使用授权书",
    "原创性声明和使用授权书",
    // 英文（normalize 时会 lowercase + 去空白，所以这里写小写无空白形式）
    "abstract",
    "keywords",
    "keyword",
    "contents",
    "tableofcontents",
    "conclusion",
    "conclusions",
    "references",
    "bibliography",
    "acknowledgement",
    "acknowledgements",
    "acknowledgment",
    "acknowledgments",
    "appendix",
    "appendices",
    "index",
];

/// 加载非编号章节白名单。
///
/// 解析与降级策略与 [`a_anti_ai::load_blackwords`] 完全对称：
/// - 优先读 `thesis_dir/non_numbered_headings.txt`（每行一个，`#` 开头为注释）
/// - 文件不存在 → 静默用内置 `EXEMPT_HEADING_NAMES`
/// - 文件存在但不可读 → `tracing::warn!` 记录 + 降回内置
///
/// # 参数
/// - `thesis_dir`：通常是 docx 同级的 `.thesis/` 目录，`None` 时直接返回内置列表。
#[must_use]
pub fn load_exempt_headings(thesis_dir: Option<&Path>) -> Vec<String> {
    if let Some(dir) = thesis_dir {
        let path = dir.join(EXEMPT_HEADINGS_FILENAME);
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let names: Vec<String> = content
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty() && !line.starts_with('#'))
                    .map(str::to_owned)
                    .collect();
                if !names.is_empty() {
                    return names;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // 文件不存在 → 用内置兜底
            }
            Err(e) => {
                warn!(
                    error = ?e,
                    path = %path.display(),
                    "non_numbered_headings.txt 不可读，降回内置白名单"
                );
            }
        }
    }
    EXEMPT_HEADING_NAMES
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
}

/// 章节标题归一化：去掉所有 Unicode 空白（含全角空格 U+3000）+ ASCII 小写化。
///
/// 例：`"摘  要"` / `"摘　要"` / `"摘要"` 三者归一化后都是 `"摘要"`；
/// `"ABSTRACT"` / `"Abstract"` / `"  abstract  "` 都归一化为 `"abstract"`。
fn normalize_heading_text(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// 判断段落文本是否匹配非编号章节白名单。
///
/// 匹配规则：段落归一化后**以**任一白名单条目（同样归一化）开头。
/// 用前缀匹配是为了覆盖 `"附录 A 调查问卷"` / `"附录A"` / `"AppendixA"` 这类带后缀的标题。
fn is_exempt_heading(text: &str, exempt: &[String]) -> bool {
    let norm_text = normalize_heading_text(text);
    if norm_text.is_empty() {
        return false;
    }
    exempt.iter().any(|w| {
        let norm_w = normalize_heading_text(w);
        !norm_w.is_empty() && norm_text.starts_with(&norm_w)
    })
}

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

/// 判断 lvlText 是否为合法的章节编号模式。
///
/// 合法模式示例：
/// - `"%1."` — 一级章节（1. 2. 3.）
/// - `"%1.%2"` — 二级章节（1.1 1.2）
/// - `"%1.%2.%3"` — 三级章节
///
/// 不合法：空字符串、纯固定文字（无 % 占位符）。
fn is_chapter_lvl_text(lvl_text: &str) -> bool {
    !lvl_text.is_empty() && lvl_text.contains('%')
}

/// 判断 lvlText 是否为合法的参考文献编号模式。
///
/// 合法模式示例：
/// - `"[%1]"` — 参考文献标准格式
///
/// 不合法：空字符串、`"%1."` 等章节格式。
fn is_reference_lvl_text(lvl_text: &str) -> bool {
    lvl_text.contains("[%") && lvl_text.contains(']')
}

/// E.5.7：检查章节标题是否使用了 Word 自动编号。
///
/// 命中条件（满足其一）：
/// 1. 段落样式 ID 是标题样式 且 无 numPr
/// 2. 段落文本以手动章节号模式开头（不依赖样式）且 无 numPr
///
/// 当提供 `numbering_map` 时，额外校验：
/// - 有 numPr 的标题段落，若其 numId 对应的 lvlText 不符合章节编号格式，也产生违规。
///
/// **白名单豁免**：摘要 / ABSTRACT / 目录 / 结论 / 参考文献 / 致谢 / 附录 / 索引
/// 等非编号章节按 GB/T 7714 与高校论文规范不参与正文自动编号，整段跳过本规则。
/// 详见 [`EXEMPT_HEADING_NAMES`] 与 [`load_exempt_headings`]。
///
/// 参数 `numbering_map` 为 None 时行为等同于原有逻辑（保持兼容）。
/// 参数 `exempt_headings` 传 `&[]` 时禁用白名单（行为退化到原有逻辑）。
#[must_use]
pub fn check_e57_chapter_numbering(
    paragraphs: &[DocParagraph],
    numbering_map: Option<&[NumIdLvlTexts]>,
    exempt_headings: &[String],
) -> Vec<Violation> {
    let mut violations = Vec::new();

    for para in paragraphs {
        let is_heading = para.style_id.as_deref().is_some_and(is_heading_style);

        // 白名单豁免：仅对标题样式生效（防止正文段落以"摘要"开头被误豁免）
        if is_heading && is_exempt_heading(&para.text, exempt_headings) {
            continue;
        }

        let manual_prefix = looks_like_manual_chapter_number(&para.text);

        if (is_heading || manual_prefix) && !para.has_num_pr {
            // 无 numPr：明确违规
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
        } else if is_heading && para.has_num_pr {
            // 有 numPr + 标题样式：若提供了编号映射则验证 lvlText 格式
            if let (Some(map), Some(num_id), Some(ilvl)) = (numbering_map, para.num_id, para.ilvl) {
                let entry = map.iter().find(|n| n.num_id == num_id);
                if let Some(entry) = entry {
                    // ilvl 语义上为非负值，用 try_from 安全转换
                    let lvl_idx = usize::try_from(ilvl).unwrap_or(0);
                    let lvl_text = entry.lvl_texts.get(lvl_idx).map_or("", String::as_str);
                    if !lvl_text.is_empty() && !is_chapter_lvl_text(lvl_text) {
                        violations.push(Violation {
                            rule_id: RuleId::E57,
                            severity: Severity::Critical,
                            location: format!("body/p[{}]", para.index),
                            actual: format!(
                                "标题段落 numPr numId={num_id} ilvl={ilvl} 对应 lvlText=\"{lvl_text}\" 不符合章节编号格式（应含 % 占位符）"
                            ),
                        });
                    }
                }
            }
        }
    }

    violations
}

/// E.5.8：参考文献条目自动编号检查（部分实现）。
///
/// 完整实现依赖 `numbering.rs` 中对 `NumberingPart` 的解析。
/// 此处检测：段落文本是否以 "[数字]" 开头（手动编号特征）但无 numPr。
///
/// 当提供 `numbering_map` 时，额外校验：
/// - 有 numPr 的参考文献段落，若其 numId 对应的 lvlText 不是 `[%N]` 格式，也产生违规。
///
/// 参数 `numbering_map` 为 None 时行为等同于原有逻辑（保持兼容）。
#[must_use]
pub fn check_e58_reference_numbering(
    paragraphs: &[DocParagraph],
    numbering_map: Option<&[NumIdLvlTexts]>,
) -> Vec<Violation> {
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
        } else if para.has_num_pr && looks_like_manual_ref_number(&para.text) {
            // 有 numPr + 文本像参考文献：若提供了编号映射则验证 lvlText
            if let (Some(map), Some(num_id), Some(ilvl)) = (numbering_map, para.num_id, para.ilvl) {
                let entry = map.iter().find(|n| n.num_id == num_id);
                if let Some(entry) = entry {
                    let lvl_idx = usize::try_from(ilvl).unwrap_or(0);
                    let lvl_text = entry.lvl_texts.get(lvl_idx).map_or("", String::as_str);
                    if !lvl_text.is_empty() && !is_reference_lvl_text(lvl_text) {
                        violations.push(Violation {
                            rule_id: RuleId::E58,
                            severity: Severity::Critical,
                            location: format!("body/p[{}]", para.index),
                            actual: format!(
                                "参考文献 numPr numId={num_id} ilvl={ilvl} 对应 lvlText=\"{lvl_text}\" 不符合 [%N] 格式"
                            ),
                        });
                    }
                }
            }
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

    fn make_para_with_num(
        index: usize,
        text: &str,
        num_id: i32,
        ilvl: i64,
        style_id: Option<&str>,
    ) -> DocParagraph {
        DocParagraph {
            index,
            text: text.to_owned(),
            has_num_pr: true,
            num_id: Some(num_id),
            ilvl: Some(ilvl),
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
        let v = check_e57_chapter_numbering(&paras, None, &[]);
        assert!(!v.is_empty(), "Heading1 无 numPr 应违规");
        assert_eq!(v[0].rule_id, RuleId::E57);
        assert_eq!(v[0].severity, Severity::Critical);
    }

    #[test]
    fn test_e57_manual_prefix_without_num_pr_is_violation() {
        // 手动键入 "1. 引言" 且无 numPr → 违规
        let paras = vec![make_para(2, "1. 引言", false, None)];
        let v = check_e57_chapter_numbering(&paras, None, &[]);
        assert!(!v.is_empty(), "手动章节号段落无 numPr 应违规");
        assert_eq!(v[0].location, "body/p[2]");
    }

    #[test]
    fn test_e57_clean_heading_with_num_pr_passes() {
        // Heading1 + 有 numPr → 通过
        let paras = vec![make_para(0, "引言", true, Some("Heading1"))];
        let v = check_e57_chapter_numbering(&paras, None, &[]);
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
        let v = check_e57_chapter_numbering(&paras, None, &[]);
        assert!(v.is_empty(), "普通正文段落不应触发 E.5.7");
    }

    #[test]
    fn test_e57_two_level_manual_number_detected() {
        // "2.3 实验设计" 手动二级编号 → 违规
        let paras = vec![make_para(5, "2.3 实验设计", false, None)];
        let v = check_e57_chapter_numbering(&paras, None, &[]);
        assert!(!v.is_empty(), "二级手动章节号应触发 E.5.7");
    }

    #[test]
    fn test_e57_with_valid_numbering_map_passes() {
        // 有 numPr + lvlText="%1." → 合法章节编号，不违规
        let paras = vec![make_para_with_num(0, "引言", 1, 0, Some("Heading1"))];
        let map = vec![NumIdLvlTexts {
            num_id: 1,
            lvl_texts: vec!["%1.".to_owned()],
        }];
        let v = check_e57_chapter_numbering(&paras, Some(&map), &[]);
        assert!(v.is_empty(), "lvlText=%1. 应为合法章节编号，不违规");
    }

    #[test]
    fn test_e57_with_invalid_lvl_text_fires() {
        // 有 numPr + lvlText="一" (固定文字，无占位符) → 不符合章节格式，违规
        let paras = vec![make_para_with_num(0, "引言", 2, 0, Some("Heading1"))];
        let map = vec![NumIdLvlTexts {
            num_id: 2,
            lvl_texts: vec!["一".to_owned()],
        }];
        let v = check_e57_chapter_numbering(&paras, Some(&map), &[]);
        assert!(!v.is_empty(), "lvlText='一' 不符合章节格式应违规");
        assert_eq!(v[0].rule_id, RuleId::E57);
    }

    // ========================
    // E.5.8 测试
    // ========================

    #[test]
    fn test_e58_manual_ref_number_detected() {
        // "[1] 作者, 标题..." 手动编号 → 违规
        let paras = vec![make_para(0, "[1] Smith J. Research. 2023.", false, None)];
        let v = check_e58_reference_numbering(&paras, None);
        assert!(!v.is_empty(), "手动参考文献编号应触发 E.5.8");
        assert_eq!(v[0].rule_id, RuleId::E58);
    }

    #[test]
    fn test_e58_auto_ref_passes() {
        // 有 numPr 的参考文献段落 → 通过（无编号映射时不进一步检查 lvlText）
        let paras = vec![make_para(0, "[1] Smith J. Research. 2023.", true, None)];
        let v = check_e58_reference_numbering(&paras, None);
        assert!(v.is_empty(), "有 numPr 的参考文献不应违规（无 map）");
    }

    #[test]
    fn test_e58_with_correct_lvl_text_passes() {
        // 有 numPr + lvlText="[%1]" → 合法参考文献编号
        let paras = vec![make_para_with_num(
            0,
            "[1] Smith J. Research. 2023.",
            3,
            0,
            None,
        )];
        let map = vec![NumIdLvlTexts {
            num_id: 3,
            lvl_texts: vec!["[%1]".to_owned()],
        }];
        let v = check_e58_reference_numbering(&paras, Some(&map));
        assert!(v.is_empty(), "lvlText=[%1] 是合法参考文献格式，不违规");
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

    // ========================
    // 非编号章节白名单测试（修复用户报告的 E.5.7 硬误报）
    // ========================

    fn builtin_exempt() -> Vec<String> {
        load_exempt_headings(None)
    }

    #[test]
    fn test_normalize_heading_text_handles_whitespace_and_case() {
        // 中文版式空格：半角 / 全角 / 多个空格 — 归一化后都应等于无空格
        assert_eq!(normalize_heading_text("摘  要"), "摘要");
        assert_eq!(normalize_heading_text("摘　要"), "摘要");
        assert_eq!(normalize_heading_text(" 摘要 "), "摘要");
        // 英文大小写
        assert_eq!(normalize_heading_text("ABSTRACT"), "abstract");
        assert_eq!(normalize_heading_text("Abstract"), "abstract");
        assert_eq!(normalize_heading_text("  abstract  "), "abstract");
        // 混合
        assert_eq!(
            normalize_heading_text("Acknowledgements"),
            "acknowledgements"
        );
    }

    #[test]
    fn test_is_exempt_heading_exact_match() {
        let exempt = builtin_exempt();
        assert!(is_exempt_heading("摘要", &exempt));
        assert!(is_exempt_heading("致谢", &exempt));
        assert!(is_exempt_heading("参考文献", &exempt));
        assert!(is_exempt_heading("目录", &exempt));
        assert!(is_exempt_heading("结论", &exempt));
        // 英文
        assert!(is_exempt_heading("ABSTRACT", &exempt));
        assert!(is_exempt_heading("References", &exempt));
        assert!(is_exempt_heading("Acknowledgements", &exempt));
    }

    #[test]
    fn test_is_exempt_heading_whitespace_variants() {
        let exempt = builtin_exempt();
        // 用户截图里出现的版式空格写法
        assert!(is_exempt_heading("摘  要", &exempt), "半角双空格应豁免");
        assert!(is_exempt_heading("致　谢", &exempt), "全角空格应豁免");
        assert!(is_exempt_heading("目  录", &exempt));
    }

    #[test]
    fn test_is_exempt_heading_prefix_match_covers_appendix_suffix() {
        let exempt = builtin_exempt();
        // 附录 + 编号后缀的常见写法
        assert!(is_exempt_heading("附录A", &exempt));
        assert!(is_exempt_heading("附录 A 调查问卷", &exempt));
        assert!(is_exempt_heading("Appendix A", &exempt));
        // 攻读学位期间... 这类长标题
        assert!(is_exempt_heading("攻读硕士学位期间发表的学术论文", &exempt));
    }

    #[test]
    fn test_is_exempt_heading_normal_text_not_matched() {
        let exempt = builtin_exempt();
        // 正文段落不应被白名单命中
        assert!(!is_exempt_heading("本研究采用对比实验方法", &exempt));
        assert!(!is_exempt_heading("第一章 引言", &exempt));
        assert!(!is_exempt_heading("1. 引言", &exempt));
        assert!(!is_exempt_heading("", &exempt));
    }

    #[test]
    fn test_e57_exempt_chinese_headings_with_heading_style_pass() {
        // 用户报告的 6 段：摘要 / ABSTRACT / 目录 / 结论 / 参考文献 / 致谢
        // Heading1 样式 + 无 numPr，按规范不应触发 E.5.7
        let exempt = builtin_exempt();
        let paras = vec![
            make_para(0, "摘要", false, Some("Heading1")),
            make_para(1, "ABSTRACT", false, Some("Heading1")),
            make_para(2, "目录", false, Some("Heading1")),
            make_para(3, "结论", false, Some("Heading1")),
            make_para(4, "参考文献", false, Some("Heading1")),
            make_para(5, "致谢", false, Some("Heading1")),
        ];
        let v = check_e57_chapter_numbering(&paras, None, &exempt);
        assert!(
            v.is_empty(),
            "用户报告的 6 个非编号章节不应触发 E.5.7，实际：{v:?}"
        );
    }

    #[test]
    fn test_e57_exempt_with_whitespace_variants_pass() {
        // 用户截图里实际文本含版式空格的写法
        let exempt = builtin_exempt();
        let paras = vec![
            make_para(0, "摘  要", false, Some("Heading1")),
            make_para(1, "目  录", false, Some("Heading1")),
            make_para(2, "致　谢", false, Some("Heading1")),
        ];
        let v = check_e57_chapter_numbering(&paras, None, &exempt);
        assert!(v.is_empty(), "版式空格变体不应触发 E.5.7：{v:?}");
    }

    #[test]
    fn test_e57_appendix_with_suffix_passes() {
        // "附录 A 调查问卷" Heading1 无 numPr → 豁免
        let exempt = builtin_exempt();
        let paras = vec![
            make_para(0, "附录 A 调查问卷", false, Some("Heading1")),
            make_para(1, "附录B", false, Some("Heading1")),
        ];
        let v = check_e57_chapter_numbering(&paras, None, &exempt);
        assert!(v.is_empty(), "附录章节带后缀不应触发 E.5.7：{v:?}");
    }

    #[test]
    fn test_e57_exempt_does_not_affect_real_chapter_heading() {
        // 白名单不应误豁免真正的正文章节
        let exempt = builtin_exempt();
        let paras = vec![make_para(0, "第一章 引言", false, Some("Heading1"))];
        let v = check_e57_chapter_numbering(&paras, None, &exempt);
        assert!(!v.is_empty(), "正文章节'第一章 引言'仍应触发 E.5.7");
    }

    #[test]
    fn test_e57_empty_exempt_list_disables_whitelist() {
        // 传 &[] 时白名单失效，行为退化到原有逻辑 — 摘要 Heading1 仍报警
        let paras = vec![make_para(0, "摘要", false, Some("Heading1"))];
        let v = check_e57_chapter_numbering(&paras, None, &[]);
        assert!(
            !v.is_empty(),
            "空白名单时应退化到原行为：摘要 Heading1 应触发 E.5.7"
        );
    }

    #[test]
    fn test_e57_normal_text_starting_with_exempt_word_not_falsely_exempted() {
        // 正文段落"摘要部分应当...", 普通样式（非 Heading）, 不应被白名单豁免
        // 同时也不应被 E.5.7 命中（普通段落本来就不在 E.5.7 范围）
        let exempt = builtin_exempt();
        let paras = vec![make_para(
            0,
            "摘要部分应当简明扼要地概括研究内容",
            false,
            None,
        )];
        let v = check_e57_chapter_numbering(&paras, None, &exempt);
        assert!(v.is_empty(), "普通正文段落（非 Heading）本就不该触发 E.5.7");
    }

    #[test]
    fn test_load_exempt_headings_none_returns_builtin() {
        // None → 内置列表非空
        let names = load_exempt_headings(None);
        assert!(!names.is_empty(), "None 时应返回内置白名单");
        assert!(names.iter().any(|w| w == "摘要"));
        assert!(names.iter().any(|w| w == "参考文献"));
        assert!(names.iter().any(|w| w == "致谢"));
    }

    #[test]
    fn test_load_exempt_headings_missing_file_returns_builtin() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let names = load_exempt_headings(Some(tmp_dir.path()));
        assert_eq!(
            names.len(),
            EXEMPT_HEADING_NAMES.len(),
            "无配置文件时应返回内置列表"
        );
    }

    #[test]
    fn test_load_exempt_headings_from_file_overrides_builtin() {
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let path = tmp_dir.path().join(EXEMPT_HEADINGS_FILENAME);
        std::fs::write(&path, "# 自定义白名单\n自定义前置章节\n\n自定义后置章节\n").unwrap();

        let names = load_exempt_headings(Some(tmp_dir.path()));
        assert_eq!(names.len(), 2, "应只解析出 2 个有效条目，实际：{names:?}");
        assert!(names.contains(&"自定义前置章节".to_owned()));
        assert!(names.contains(&"自定义后置章节".to_owned()));
        // 内置列表中的"摘要"应已被覆盖
        assert!(
            !names.contains(&"摘要".to_owned()),
            "用户文件存在且非空时应完全覆盖内置列表"
        );
    }
}
