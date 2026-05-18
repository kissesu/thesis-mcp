//! @file tools/audit_format.rs
//! @description 把 `AuditResult` 失败诊断格式化为对用户友好的多行中文文本
//!
//! 背景：旧版 `revise.rs` 与 `write_section.rs` 拒绝写入时只输出
//! "审计未通过：N 条 Critical 违规"。AuditResult 内已含
//! self_check_table（规则编号 + 人读名 + 期望 + 实际 + 位置）但被丢弃。
//!
//! 新版输出格式（示例）：
//!
//! ```text
//! 审计未通过，2 条 Critical 规则被命中（共 3 处），原文件未修改。
//!
//! 【E.5.7 章节号自动编号】2 处
//!   期望：章节标题段落含 numPr（自动编号）
//!   实际：标题样式段落（Heading1）无 numPr 自动编号；...
//!   位置：body/p[3], body/p[15]
//!   建议：章节标题缺自动编号。两种修法：
//!     (1) 在 Word 中给标题应用列表自动编号；
//!     (2) 如果是摘要/目录/致谢等非编号章节，把章节名加入
//!         docx 同级 .thesis/non_numbered_headings.txt 白名单。
//!
//! 【E.5.8 参考文献自动编号】1 处
//!   ...
//! ```
//!
//! @author Atlas.oi
//! @date 2026-05-18

use std::fmt::Write as _;

use thesis_types::{AuditResult, RuleId, Severity};

/// 每条规则展示的位置上限（超过则截断 + 省略提示）
const MAX_LOCATIONS_PER_RULE: usize = 5;

/// actual 字段截断字符数（避免单条规则吃掉整段错误消息）
const MAX_ACTUAL_CHARS: usize = 160;

/// 把 `AuditResult`（passed=false）格式化为多行中文诊断文本。
///
/// 仅处理 Critical 级别的命中行（与 `audit_full` 内 `passed` 判定口径一致）。
/// 若 `self_check_table` 中没有 Critical 行（理论上不应发生），返回保底文本
/// 含原始 violations_count 数字。
#[must_use]
pub fn format_audit_failure(audit_result: &AuditResult) -> String {
    let critical_rows: Vec<_> = audit_result
        .self_check_table
        .iter()
        .filter(|r| !r.passed && matches!(r.severity, Severity::Critical))
        .collect();

    if critical_rows.is_empty() {
        return format!(
            "检查没过：{} 处问题，但内部记录里没拿到具体明细（数据对不上，可能是 bug）；原文件没改。",
            audit_result.violations_count
        );
    }

    let total_hits: usize = critical_rows.iter().map(|r| r.locations.len()).sum();

    let mut msg = format!(
        "检查没过：发现 {} 类严重问题（共 {} 处），原文件没改。\n",
        critical_rows.len(),
        total_hits
    );

    // write! 写入 String 永不失败，unwrap 安全（标准库保证）
    for row in &critical_rows {
        writeln!(
            msg,
            "\n【{} {}】{} 处",
            row.rule_id.as_str(),
            row.item,
            row.locations.len()
        )
        .unwrap();
        writeln!(msg, "  应该是：{}", translate_jargon(&row.expected)).unwrap();

        let actual_brief = truncate_chars(&row.actual, MAX_ACTUAL_CHARS);
        writeln!(msg, "  现在是：{}", translate_jargon(&actual_brief)).unwrap();

        let translated_locs: Vec<String> = row
            .locations
            .iter()
            .map(|l| translate_location(l))
            .collect();
        let loc_str = if translated_locs.len() <= MAX_LOCATIONS_PER_RULE {
            translated_locs.join("、")
        } else {
            let shown = &translated_locs[..MAX_LOCATIONS_PER_RULE];
            format!(
                "{}、... 还有 {} 处",
                shown.join("、"),
                translated_locs.len() - MAX_LOCATIONS_PER_RULE
            )
        };
        writeln!(msg, "  在哪：{loc_str}").unwrap();

        msg.push_str("  怎么改：");
        msg.push_str(rule_action_hint(row.rule_id));
        msg.push('\n');
    }

    msg.push_str("\n改完后请再次调用 mcp__thesis__write_section 或 mcp__thesis__revise。");

    msg
}

/// 把规则函数原始输出里的内部字段名（OOXML 与代码层术语）换成中文白话。
///
/// 用于 `expected` 与 `actual` 字段。新增字段名翻译时在此扩展。
/// 顺序很重要：长字符串优先匹配，避免被短字符串先吃掉。
fn translate_jargon(s: &str) -> String {
    // (原文 → 译文) 按长度降序排列
    const PAIRS: &[(&str, &str)] = &[
        ("自动编号 ([%1] 格式)", "自动编号（编号显示为 [1] [2] [3]）"),
        ("firstLineChars", "首行缩进（按字数）"),
        ("leftChars", "左缩进（按字数）"),
        ("Heading1", "1 级标题"),
        ("Heading2", "2 级标题"),
        ("Heading3", "3 级标题"),
        ("Heading4", "4 级标题"),
        ("Heading5", "5 级标题"),
        ("heading1", "1 级标题"),
        ("heading2", "2 级标题"),
        ("heading3", "3 级标题"),
        ("lvlText", "编号显示格式"),
        ("numPr", "自动编号设置"),
        ("numId", "编号 ID"),
        ("ilvl", "编号级别"),
        ("[%N]", "[1] [2] [3] 这种自动编号"),
        ("[%1]", "[1] [2] [3] 这种自动编号"),
        ("%1.%2.%3", "1.1.1 / 2.3.1 这类三级编号"),
        ("%1.%2", "1.1 / 2.3 这类二级编号"),
        ("%1.", "1. / 2. / 3. 这类一级编号"),
        ("firstLine", "首行缩进"),
    ];

    let mut out = s.to_owned();
    for (from, to) in PAIRS {
        out = out.replace(from, to);
    }
    out
}

/// 把内部位置字符串（如 `body/p[3]` / `tbl[2]/tr[0]/tc[1]/p[0]`）换成
/// 用户能直接定位的中文描述（如 `正文第 4 段` / `第 3 个表格第 1 行第 2 列第 1 段`）。
///
/// docx 段落索引从 0 开始，中文叙述习惯从 1 数起，所以 +1。
fn translate_location(loc: &str) -> String {
    // 表格位置：tbl[T]/tr[R]/tc[C]/p[P]
    if let Some(rest) = loc.strip_prefix("tbl[")
        && let Some((t_idx, after_t)) = parse_index_and_rest(rest, "]/tr[")
        && let Some((r_idx, after_r)) = parse_index_and_rest(after_t, "]/tc[")
        && let Some((c_idx, after_c)) = parse_index_and_rest(after_r, "]/p[")
        && let Some((p_idx, _)) = parse_index_and_rest(after_c, "]")
    {
        return format!(
            "第 {} 个表格第 {} 行第 {} 列第 {} 段",
            t_idx + 1,
            r_idx + 1,
            c_idx + 1,
            p_idx + 1
        );
    }

    // 正文段落：body/p[N]
    if let Some(rest) = loc.strip_prefix("body/p[")
        && let Some((p_idx, _)) = parse_index_and_rest(rest, "]")
    {
        return format!("正文第 {} 段", p_idx + 1);
    }

    // 未识别的位置格式 → 原样返回（兜底，不丢信息）
    loc.to_owned()
}

/// 从字符串开头解析一个数字，直到遇到分隔符 `sep`，返回 (数字, sep 之后的剩余字符串)。
fn parse_index_and_rest<'a>(s: &'a str, sep: &str) -> Option<(usize, &'a str)> {
    let sep_pos = s.find(sep)?;
    let num: usize = s[..sep_pos].parse().ok()?;
    Some((num, &s[sep_pos + sep.len()..]))
}

/// 按 `RuleId` 返回一段针对性的中文可行动建议。
///
/// 新增规则时必须更新此函数；编译期 match 穷尽性会帮你发现漏 case。
fn rule_action_hint(rule_id: RuleId) -> &'static str {
    match rule_id {
        RuleId::A1 => "段落里有 AI 爱用的套话词，删掉或换成更自然的中文说法。",
        RuleId::A5 => {
            "长破折号 — 用错了。只在表示「停顿、转折、解释」时用，\
             其他场合（如表示范围）改用其他标点。"
        }
        RuleId::A6 => "中文字符和英文/数字挨着时漏了空格。在两者之间补一个空格。",
        RuleId::A7 => "英文单词跟中文或数字挨着时漏了空格。在英文单词前后补一个空格。",
        RuleId::A9 => "括号用混了（中文（）和英文 () 都出现了），统一只用一种。",
        RuleId::C1 => {
            "引用编号 [1] 应该排成上标。在 Word 里选中那几个数字，\
             按 Ctrl + Shift + 等号（三个键一起按）就变成上标了。"
        }
        RuleId::C2 => {
            "参考文献的引用编号顺序乱了。按论文里第一次出现的先后顺序重新编（\
             [1] 必须先出现，[2] 才能跟在后面）。"
        }
        RuleId::D91 | RuleId::D92 => {
            "表格里的段落带了缩进。选中整个表格，右键打开「段落」对话框，\
             把「首行缩进」和「左侧缩进」都改成 0。"
        }
        RuleId::E57 => {
            "标题段落没用 Word 的自动编号。两种改法：\n    \
             (1) 推荐：在 Word 里选中标题，从「开始」→「编号」按钮里挑一个\
             带数字的样式（比如「1. 」或「1.1 」）应用上去。\n    \
             (2) 如果这条是误报（摘要/目录/致谢/参考文献/附录等本来就不该有编号），\n        \
             在论文同目录下建一个 .thesis 文件夹，里面建一个 \
             non_numbered_headings.txt 文件，把这个标题的名字写进去（每行一个，# 开头是注释）。\n        \
             内置已经豁免了：摘要、ABSTRACT、目录、结论、参考文献、致谢、附录、索引 等。\n        \
             还报警一般是因为你的标题名跟内置条目对不上（比如学校用了自定义名字）。"
        }
        RuleId::E58 => {
            "参考文献列表的编号是手动打上去的（比如自己敲了 [1] [2]）。\
             在 Word 里选中整个参考文献列表，从「开始」→「编号」里挑「[1] [2] [3]」样式应用。"
        }
        RuleId::F51 => {
            "文档里有没接受的修订痕迹（红色的删除线、下划线等）。\
             在 Word 顶部菜单选「审阅」→「接受」→「接受所有修订」。"
        }
        RuleId::F52 => {
            "文档里还留着批注（右侧的气泡评论）。\
             在 Word 顶部菜单选「审阅」→「删除」→「删除文档中的所有批注」。"
        }
    }
}

/// 按字符数（不是字节）截断字符串，超出补 …。
///
/// 避免在 UTF-8 字符边界中间切断造成乱码。
fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_owned();
    }
    let head: String = s.chars().take(max_chars).collect();
    format!("{head}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::path::PathBuf;
    use thesis_types::CheckRow;

    fn make_audit_result(rows: Vec<CheckRow>, violations_count: usize) -> AuditResult {
        AuditResult {
            docx_path: PathBuf::from("/tmp/test.docx"),
            sha256_hex: "0".repeat(64),
            audited_at: Utc::now(),
            audit_version: "test".to_owned(),
            passed: violations_count == 0,
            violations_count,
            self_check_table: rows,
        }
    }

    fn make_row(
        rule_id: RuleId,
        severity: Severity,
        actual: &str,
        locations: Vec<&str>,
    ) -> CheckRow {
        CheckRow {
            rule_id,
            severity,
            item: match rule_id {
                RuleId::E57 => "章节号自动编号".to_owned(),
                RuleId::C2 => "引用编号顺序".to_owned(),
                RuleId::A1 => "AI 黑词检测".to_owned(),
                _ => "测试规则".to_owned(),
            },
            expected: format!("{} 的期望状态", rule_id.as_str()),
            actual: actual.to_owned(),
            passed: false,
            locations: locations.into_iter().map(str::to_owned).collect(),
        }
    }

    #[test]
    fn test_format_includes_rule_id_item_and_location() {
        let rows = vec![make_row(
            RuleId::E57,
            Severity::Critical,
            "标题段落无 numPr",
            vec!["body/p[3]", "body/p[10]"],
        )];
        let audit = make_audit_result(rows, 2);
        let msg = format_audit_failure(&audit);

        assert!(msg.contains("E.5.7"), "应含规则编号：{msg}");
        assert!(msg.contains("章节号自动编号"), "应含人读名：{msg}");
        // 位置经过翻译：body/p[3] → 正文第 4 段；body/p[10] → 正文第 11 段
        assert!(msg.contains("正文第 4 段"), "位置应翻译为中文：{msg}");
        assert!(msg.contains("正文第 11 段"), "位置应翻译为中文：{msg}");
        assert!(msg.contains("误报"), "E.5.7 建议应提到误报豁免：{msg}");
        assert!(msg.contains("原文件没改"), "应明确告知原文件未变：{msg}");
    }

    #[test]
    fn test_format_multiple_rules_grouped() {
        let rows = vec![
            make_row(
                RuleId::E57,
                Severity::Critical,
                "无 numPr",
                vec!["body/p[0]"],
            ),
            make_row(
                RuleId::C2,
                Severity::Critical,
                "[2] 先于 [1] 出现",
                vec!["body/p[5]"],
            ),
        ];
        let audit = make_audit_result(rows, 2);
        let msg = format_audit_failure(&audit);

        // 总数行
        assert!(msg.contains("2 类严重问题"), "应显示规则数：{msg}");
        // 两条规则各自的标题
        assert!(msg.contains("【E.5.7"));
        assert!(msg.contains("【C.2"));
        // 各自的建议关键词
        assert!(msg.contains("误报"), "E.5.7 建议应提到误报豁免");
        assert!(msg.contains("第一次出现的先后"), "C.2 建议应说先后顺序");
    }

    #[test]
    fn test_format_truncates_long_actual() {
        let long_actual: String = "x".repeat(500);
        let rows = vec![make_row(
            RuleId::A1,
            Severity::Critical,
            &long_actual,
            vec!["body/p[0]"],
        )];
        let audit = make_audit_result(rows, 1);
        let msg = format_audit_failure(&audit);

        assert!(msg.contains("…"), "超长 actual 应被截断且加省略号：{msg}");
        // 截断后总长度应远小于原始的 500 + 模板，校验不爆炸
        assert!(msg.len() < 2000, "格式化输出应保持合理长度");
    }

    #[test]
    fn test_format_truncates_many_locations() {
        let locs: Vec<&str> = vec![
            "body/p[1]",
            "body/p[2]",
            "body/p[3]",
            "body/p[4]",
            "body/p[5]",
            "body/p[6]",
            "body/p[7]",
            "body/p[8]",
        ];
        let rows = vec![make_row(RuleId::E57, Severity::Critical, "...", locs)];
        let audit = make_audit_result(rows, 8);
        let msg = format_audit_failure(&audit);

        assert!(msg.contains("还有 3 处"), "超 5 处应省略：{msg}");
    }

    #[test]
    fn test_format_filters_out_warnings() {
        // A.1 是 Warning，不应进入"Critical 命中"块
        let rows = vec![
            make_row(RuleId::A1, Severity::Warning, "黑词", vec!["body/p[0]"]),
            make_row(
                RuleId::E57,
                Severity::Critical,
                "无 numPr",
                vec!["body/p[1]"],
            ),
        ];
        let audit = make_audit_result(rows, 2);
        let msg = format_audit_failure(&audit);

        assert!(msg.contains("【E.5.7"));
        assert!(
            !msg.contains("【A.1"),
            "Warning 不应出现在拒绝消息里：{msg}"
        );
        assert!(msg.contains("1 类严重问题"), "总数只算严重问题：{msg}");
    }

    #[test]
    fn test_format_returns_safe_fallback_when_no_critical() {
        // self_check_table 全是 Warning（不应发生，但容错）
        let rows = vec![make_row(
            RuleId::A1,
            Severity::Warning,
            "黑词",
            vec!["body/p[0]"],
        )];
        let audit = make_audit_result(rows, 1);
        let msg = format_audit_failure(&audit);

        // 兜底消息应说"数据对不上"或"可能是 bug"
        assert!(msg.contains("数据对不上") || msg.contains("内部记录"));
    }

    #[test]
    fn test_truncate_chars_no_panic_on_cjk() {
        let s = "中文字符串测试边界";
        let truncated = truncate_chars(s, 4);
        assert_eq!(truncated.chars().count(), 5); // 4 + 1 个 …
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn test_truncate_chars_under_max_returns_original() {
        let s = "short";
        assert_eq!(truncate_chars(s, 10), "short");
    }

    // ========================
    // 翻译函数测试
    // ========================

    #[test]
    fn test_translate_jargon_replaces_ooxml_field_names() {
        let s = "标题样式段落（Heading1）无 numPr 自动编号";
        let out = translate_jargon(s);
        assert!(!out.contains("Heading1"), "应翻译 Heading1：{out}");
        assert!(!out.contains("numPr"), "应翻译 numPr：{out}");
        assert!(out.contains("1 级标题"));
        assert!(out.contains("自动编号设置"));
    }

    #[test]
    fn test_translate_jargon_replaces_lvl_text_patterns() {
        // 复合模式应正确匹配（长串优先于短串）
        let s = "lvlText=\"%1.%2.%3\" 不符合";
        let out = translate_jargon(s);
        assert!(!out.contains("%1.%2.%3"), "三级模式应被翻译：{out}");
        assert!(out.contains("三级编号"));

        let s2 = "lvlText=\"%1.\" 应当";
        let out2 = translate_jargon(s2);
        assert!(out2.contains("一级编号"), "一级模式应被翻译：{out2}");
    }

    #[test]
    fn test_translate_jargon_leaves_plain_chinese_alone() {
        let s = "段落开头含有手动键入的章节号";
        let out = translate_jargon(s);
        assert_eq!(out, s, "纯中文不应被改动");
    }

    #[test]
    fn test_translate_location_body_paragraph() {
        assert_eq!(translate_location("body/p[0]"), "正文第 1 段");
        assert_eq!(translate_location("body/p[3]"), "正文第 4 段");
        assert_eq!(translate_location("body/p[42]"), "正文第 43 段");
    }

    #[test]
    fn test_translate_location_table_cell_paragraph() {
        assert_eq!(
            translate_location("tbl[2]/tr[0]/tc[1]/p[0]"),
            "第 3 个表格第 1 行第 2 列第 1 段"
        );
        assert_eq!(
            translate_location("tbl[0]/tr[5]/tc[3]/p[2]"),
            "第 1 个表格第 6 行第 4 列第 3 段"
        );
    }

    #[test]
    fn test_translate_location_unknown_format_returns_original() {
        // 未识别的格式 → 原样返回，不丢信息
        assert_eq!(translate_location("foo/bar"), "foo/bar");
        assert_eq!(translate_location("headers/h1/p[0]"), "headers/h1/p[0]");
    }

    #[test]
    fn test_rule_action_hint_all_variants_have_text() {
        // 确保所有 RuleId 都有非空建议（match 穷尽性 + 内容非空）
        for rid in [
            RuleId::A1,
            RuleId::A5,
            RuleId::A6,
            RuleId::A7,
            RuleId::A9,
            RuleId::C1,
            RuleId::C2,
            RuleId::D91,
            RuleId::D92,
            RuleId::E57,
            RuleId::E58,
            RuleId::F51,
            RuleId::F52,
        ] {
            let hint = rule_action_hint(rid);
            assert!(!hint.is_empty(), "RuleId {} 缺少建议文案", rid.as_str());
        }
    }
}
