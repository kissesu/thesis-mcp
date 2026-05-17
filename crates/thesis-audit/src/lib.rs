//! @file lib.rs
//! @description thesis-audit 核心审计引擎公开 API
//!
//! 对外暴露两个主要函数：
//! - `audit_full`：对整个 docx 执行全量规则检查
//! - `audit_section`：针对特定节（section_id）执行检查（stub，待 L2.1b）
//!
//! 执行流程（audit_full）：
//! 1. 读取 docx 文件字节，计算 sha256
//! 2. 用 `document::Document::load` 加载段落
//! 3. 逐条规则调用 rules/*.rs 检查函数，收集 `Violation`
//! 4. 将 `Vec<Violation>` 按 rule_id 聚合为 `Vec<CheckRow>`
//! 5. 构造 `AuditResult`：passed = 无 Critical 违规
//!
//! @author Atlas.oi
//! @date 2026-05-17

pub mod document;
pub mod error;
pub mod rules;

// Priority 2 (部分实现)
pub mod numbering;
pub mod tables;

// Priority 3 stubs
pub mod comments;
pub mod footnotes;
pub mod headers_footers;
pub mod styles;
pub mod textbox;
pub mod tracked_changes;

pub use error::AuditError;

use std::collections::HashMap;
use std::path::Path;

use chrono::Utc;
use sha2::{Digest, Sha256};
use thesis_types::{AuditResult, CheckRow, RuleId, Severity};
use tracing::debug;

use crate::document::Document;
use crate::rules::{Violation, a_anti_ai, e_format};

/// 审计引擎版本，与 Cargo.toml 保持一致。
const AUDIT_VERSION: &str = env!("CARGO_PKG_VERSION");

// ============================================
// 公开 API
// ============================================

/// 对整个 docx 执行全量规则检查。
///
/// 业务流程：
/// 1. 读取文件字节 + 计算 sha256
/// 2. 加载文档段落
/// 3. 执行 PRIORITY 1 规则（A.1, E.5.7, E.5.8）
/// 4. 聚合 Violation → CheckRow → AuditResult
pub fn audit_full(docx_path: &Path) -> Result<AuditResult, AuditError> {
    debug!("开始全量审计：{:?}", docx_path);

    // ============================================
    // 第一步：读取文件，计算 sha256
    // ============================================
    let docx_bytes = std::fs::read(docx_path)?;
    let sha256_hex = compute_sha256(&docx_bytes);

    // ============================================
    // 第二步：加载文档
    // ============================================
    let doc = Document::load(docx_path)?;
    debug!("提取段落数：{}", doc.paragraphs.len());

    // ============================================
    // 第三步：执行规则检查，收集 Violation
    // ============================================
    let mut all_violations: Vec<Violation> = Vec::new();

    // A.1：黑词检测
    all_violations.extend(a_anti_ai::check_a1_blackwords(&doc.paragraphs));

    // E.5.7：章节号自动编号检测
    all_violations.extend(e_format::check_e57_chapter_numbering(&doc.paragraphs));

    // E.5.8：参考文献自动编号检测（部分实现）
    all_violations.extend(e_format::check_e58_reference_numbering(&doc.paragraphs));

    // ============================================
    // 第四步：按 rule_id 聚合 Violation → CheckRow
    // ============================================
    let check_rows = aggregate_violations(all_violations);

    // ============================================
    // 第五步：构造 AuditResult
    // passed = 没有任何 Critical 级别的违规
    // ============================================
    let violations_count = check_rows
        .iter()
        .filter(|r| !r.passed && matches!(r.severity, Severity::Critical | Severity::Warning))
        .count();

    let passed = check_rows
        .iter()
        .all(|r| r.passed || !matches!(r.severity, Severity::Critical));

    Ok(AuditResult {
        docx_path: docx_path.to_path_buf(),
        sha256_hex,
        audited_at: Utc::now(),
        audit_version: AUDIT_VERSION.to_owned(),
        passed,
        violations_count,
        self_check_table: check_rows,
    })
}

/// 对特定节（section_id）执行检查（stub，待 L2.1b 实现）。
///
/// 规划：通过 section_id 定位段落范围（书签或样式边界），
/// 只对该范围内的段落执行规则检查，减少无关命中。
// stub: implement in L2.1b sub-task
pub fn audit_section(docx_path: &Path, section_id: &str) -> Result<AuditResult, AuditError> {
    todo!(
        "L2.1b: audit_section — 按 section_id({section_id:?}) 定位段落范围后调用 audit_full 子集；路径：{docx_path:?}"
    )
}

// ============================================
// 内部辅助函数
// ============================================

/// 计算字节切片的 sha256 hex 字符串。
fn compute_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// 将 `Vec<Violation>` 按 `rule_id` 聚合为 `Vec<CheckRow>`。
///
/// 每个 rule_id 只产生一行 CheckRow，多条 Violation 合并到 `locations`。
/// 无命中的规则不产生 CheckRow（调用方只收到有命中的规则行）。
fn aggregate_violations(violations: Vec<Violation>) -> Vec<CheckRow> {
    if violations.is_empty() {
        return Vec::new();
    }

    // 按 rule_id 分组
    let mut map: HashMap<RuleId, Vec<Violation>> = HashMap::new();
    for v in violations {
        map.entry(v.rule_id).or_default().push(v);
    }

    let mut rows: Vec<CheckRow> = map
        .into_iter()
        .map(|(rule_id, vs)| {
            // 取第一条的 severity（同一规则 severity 相同）
            let severity = vs[0].severity;
            let locations: Vec<String> = vs.iter().map(|v| v.location.clone()).collect();
            let actual = vs
                .iter()
                .map(|v| v.actual.as_str())
                .collect::<Vec<_>>()
                .join("；");

            CheckRow {
                rule_id,
                severity,
                item: rule_item_name(rule_id),
                expected: rule_expected_desc(rule_id),
                actual,
                passed: false,
                locations,
            }
        })
        .collect();

    // 按 rule_id 字符串排序，保证输出顺序稳定
    rows.sort_by_key(|r| r.rule_id.as_str());
    rows
}

/// 规则的人读名称。
fn rule_item_name(rule_id: RuleId) -> String {
    match rule_id {
        RuleId::A1 => "AI 黑词检测",
        RuleId::A5 => "em-dash 滥用",
        RuleId::A6 => "CJK 间距异常",
        RuleId::A7 => "英文前后空格",
        RuleId::A9 => "括号风格混用",
        RuleId::C1 => "引用标注上标",
        RuleId::C2 => "引用编号顺序",
        RuleId::D91 => "cell 缩进字符清零",
        RuleId::D92 => "cell 缩进磅值清零",
        RuleId::E57 => "章节号自动编号",
        RuleId::E58 => "参考文献自动编号",
        RuleId::F51 => "修订痕迹清除",
        RuleId::F52 => "批注清除",
    }
    .to_owned()
}

/// 规则期望状态描述。
fn rule_expected_desc(rule_id: RuleId) -> String {
    match rule_id {
        RuleId::A1 => "段落不含 AI 惯用套话词汇",
        RuleId::A5 => "em-dash 仅用于破折号场景",
        RuleId::A6 => "中英文混排时有空格间隔",
        RuleId::A7 => "英文单词前后有空格",
        RuleId::A9 => "括号风格统一（全角或半角）",
        RuleId::C1 => "引用标注 [N] 以上标形式出现",
        RuleId::C2 => "参考文献引用编号按顺序递增",
        RuleId::D91 => "表格 cell 段落 firstLineChars=0 leftChars=0",
        RuleId::D92 => "表格 cell 段落 firstLine=0 left=0",
        RuleId::E57 => "章节标题段落含 numPr（自动编号）",
        RuleId::E58 => "参考文献段落含 numPr（[%1] 格式）",
        RuleId::F51 => "文档不含未接受修订痕迹",
        RuleId::F52 => "文档不含遗留批注",
    }
    .to_owned()
}

// ============================================
// 单元测试
// ============================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::build_minimal_docx;

    /// 构建含指定文本的单段落 docx，写入临时文件，返回路径。
    fn make_docx_with_text(text: &str) -> tempfile::NamedTempFile {
        let body_xml = format!(
            r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:r><w:t>{text}</w:t></w:r>
</w:p>"#
        );
        let bytes = build_minimal_docx(&body_xml);
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &bytes).unwrap();
        tmp
    }

    #[test]
    fn test_audit_full_returns_ok_for_clean_doc() {
        let tmp = make_docx_with_text("本研究采用对比实验方法验证假设。");
        let result = audit_full(tmp.path()).expect("audit_full 应成功");
        // 无黑词、无手动章节号 → 无违规
        assert!(
            result.self_check_table.is_empty() || result.violations_count == 0,
            "干净文档不应有违规：{:?}",
            result.self_check_table
        );
        assert!(result.passed, "干净文档应通过审计");
    }

    #[test]
    fn test_audit_full_detects_blackword() {
        let tmp = make_docx_with_text("毋庸置疑，本研究具有重要价值。");
        let result = audit_full(tmp.path()).expect("audit_full 应成功");
        let a1_rows: Vec<_> = result
            .self_check_table
            .iter()
            .filter(|r| r.rule_id == RuleId::A1)
            .collect();
        assert!(!a1_rows.is_empty(), "应检测到 A.1 违规");
        assert!(!a1_rows[0].passed);
    }

    #[test]
    fn test_audit_full_sha256_is_hex_string() {
        let tmp = make_docx_with_text("测试文档");
        let result = audit_full(tmp.path()).expect("audit_full 应成功");
        // sha256 hex 应为 64 个十六进制字符
        assert_eq!(result.sha256_hex.len(), 64, "sha256 应为 64 字符 hex");
        assert!(
            result.sha256_hex.chars().all(|c| c.is_ascii_hexdigit()),
            "sha256 应全为十六进制字符"
        );
    }

    #[test]
    fn test_audit_version_non_empty() {
        let tmp = make_docx_with_text("测试");
        let result = audit_full(tmp.path()).expect("audit_full 应成功");
        assert!(!result.audit_version.is_empty());
    }

    #[test]
    fn test_error_display_io_error() {
        // 不存在的路径应返回 IoError
        let err = audit_full(Path::new("/nonexistent/path/doc.docx"));
        assert!(matches!(err, Err(AuditError::IoError(_))));
    }

    #[test]
    fn test_compute_sha256_deterministic() {
        let data = b"hello world";
        let h1 = compute_sha256(data);
        let h2 = compute_sha256(data);
        assert_eq!(h1, h2, "sha256 应是确定性的");
        assert_eq!(h1.len(), 64);
    }
}
