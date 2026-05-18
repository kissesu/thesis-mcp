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
            "审计未通过：{} 条违规，但 self_check_table 无 Critical 详情（数据不一致）；原文件未修改。",
            audit_result.violations_count
        );
    }

    let total_hits: usize = critical_rows.iter().map(|r| r.locations.len()).sum();

    let mut msg = format!(
        "审计未通过，{} 条 Critical 规则被命中（共 {} 处），原文件未修改。\n",
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
        writeln!(msg, "  期望：{}", row.expected).unwrap();

        let actual_brief = truncate_chars(&row.actual, MAX_ACTUAL_CHARS);
        writeln!(msg, "  实际：{actual_brief}").unwrap();

        let loc_str = if row.locations.len() <= MAX_LOCATIONS_PER_RULE {
            row.locations.join(", ")
        } else {
            let shown = &row.locations[..MAX_LOCATIONS_PER_RULE];
            format!(
                "{}, ... 还有 {} 处",
                shown.join(", "),
                row.locations.len() - MAX_LOCATIONS_PER_RULE
            )
        };
        writeln!(msg, "  位置：{loc_str}").unwrap();

        msg.push_str("  建议：");
        msg.push_str(rule_action_hint(row.rule_id));
        msg.push('\n');
    }

    msg.push_str("\n修复后请重新调用 mcp__thesis__write_section 或 mcp__thesis__revise。");

    msg
}

/// 按 `RuleId` 返回一段针对性的中文可行动建议。
///
/// 新增规则时必须更新此函数；编译期 match 穷尽性会帮你发现漏 case。
fn rule_action_hint(rule_id: RuleId) -> &'static str {
    match rule_id {
        RuleId::A1 => "段落含 AI 写作套话词，删除或换用更自然的中文表达。",
        RuleId::A5 => "em-dash（—）滥用，仅在表示破折号语义时使用，其他场景换标点。",
        RuleId::A6 => "中英文混排缺空格，在 CJK 字符与英文/数字之间补半角空格。",
        RuleId::A7 => "英文单词前后缺空格，与中文或数字相邻时补半角空格。",
        RuleId::A9 => "括号风格混用（中英文括号），统一为全角或半角之一。",
        RuleId::C1 => "引用 [N] 应为上标格式，在 Word 中选中数字后设为上标（Ctrl+Shift+=）。",
        RuleId::C2 => {
            "参考文献引用编号顺序错乱，按文中首次出现顺序重新编号（[1] 必须先于 [2] 出现）。"
        }
        RuleId::D91 | RuleId::D92 => {
            "表格 cell 段落缩进未清零，选中表格 → 段落属性 → 首行缩进与左缩进设为 0。"
        }
        RuleId::E57 => {
            "章节标题缺自动编号。两种修法：\n    \
             (1) 在 Word 中给标题应用列表自动编号（推荐）；\n    \
             (2) 如果是摘要/目录/致谢/参考文献/附录等非编号章节误报，\n        \
             把章节名加入 docx 同级 .thesis/non_numbered_headings.txt（每行一个，# 开头注释）。\n        \
             默认白名单已含摘要/ABSTRACT/目录/结论/参考文献/致谢/附录/索引等常见章节，\n        \
             若仍报警通常是文本与默认条目不完全匹配（如学校自定义章节名）。"
        }
        RuleId::E58 => {
            "参考文献段落缺自动编号，把参考文献列表改用 Word 的有序列表（编号格式选 [1] [2] [3]）。"
        }
        RuleId::F51 => "文档含未接受的修订痕迹，在 Word 审阅菜单中选 接受所有修订。",
        RuleId::F52 => "文档含遗留批注，在 Word 审阅菜单中选 删除所有批注。",
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
        assert!(msg.contains("body/p[3]"), "应含位置：{msg}");
        assert!(msg.contains("body/p[10]"), "应含位置：{msg}");
        assert!(msg.contains("非编号章节"), "E.5.7 建议应提到白名单：{msg}");
        assert!(msg.contains("原文件未修改"), "应明确告知原文件未变：{msg}");
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
        assert!(msg.contains("2 条 Critical 规则"), "应显示规则数：{msg}");
        // 两条规则各自的标题
        assert!(msg.contains("【E.5.7"));
        assert!(msg.contains("【C.2"));
        // 各自的建议关键词
        assert!(msg.contains("非编号章节"), "E.5.7 建议");
        assert!(msg.contains("首次出现顺序"), "C.2 建议");
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
        assert!(msg.contains("1 条 Critical"), "总数只算 Critical：{msg}");
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

        // 兜底消息应明确说"数据不一致"
        assert!(msg.contains("数据不一致") || msg.contains("无 Critical"));
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
