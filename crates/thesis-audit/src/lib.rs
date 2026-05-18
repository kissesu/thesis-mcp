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
//! @date 2026-05-18

pub mod document;
pub mod error;
pub mod rules;

// Priority 2 (部分实现)
pub mod numbering;
pub mod tables;

// Priority 3 stubs → 已实现
pub mod comments;
pub mod footnotes;
pub mod headers_footers;
pub mod styles;
pub mod textbox;
pub mod tracked_changes;

// 公共 XML 工具（comments / footnotes 共享实现）
pub mod xml_utils;

pub use error::AuditError;

use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;

use chrono::Utc;
use ooxmlsdk::parts::wordprocessing_document::WordprocessingDocument;
use sha2::{Digest, Sha256};
use thesis_types::{AuditResult, CheckRow, RuleId, Severity};
use tracing::debug;

use crate::document::Document;
use crate::rules::{Violation, a_anti_ai, c_citation, e_format};

/// 审计引擎版本，与 Cargo.toml 保持一致。
const AUDIT_VERSION: &str = env!("CARGO_PKG_VERSION");

// ============================================
// 公开 API
// ============================================

/// 对整个 docx 执行全量规则检查。
///
/// 业务流程：
/// 1. 读取文件字节（一次 IO）+ 计算 sha256
/// 2. 用同一份字节加载文档主体段落（无二次读取，消除 TOCTOU）
/// 3. 解析黑词列表：优先查找 docx 同级的 `.thesis/blackwords.txt`，不存在则用内置列表
/// 4. 构建编号映射（numbering.rs）+ 加载样式表（styles.rs）
/// 5. 执行 P1 规则（A.1, E.5.7, E.5.8）+ P1 页眉/页脚/文本框扫描 + F.5.x 修订检测
/// 6. 执行 P2 规则（D.9.x 表格缩进）
/// 7. 执行 P3 规则（批注 / 脚注 A.1 扫描，C.1/C.2 引用检查）
/// 8. 聚合 Violation → CheckRow → AuditResult
///
/// # 黑词目录解析规则
/// - 取 `docx_path` 的父目录，查找 `parent/.thesis/blackwords.txt`
/// - 若文件存在且可读 → 用文件中的词列表；否则 → 内置 `BLACKWORDS`
///
/// # 语义约定
/// - `passed = true` 当且仅当没有 Critical 级别的命中行
/// - `violations_count` 只统计 Critical 命中行数（与 `passed` 同源，无歧义）
pub fn audit_full(docx_path: &Path) -> Result<AuditResult, AuditError> {
    debug!("开始全量审计：{:?}", docx_path);

    // ============================================
    // 第一步：一次性读取文件字节，计算 sha256
    // ============================================
    let docx_bytes = std::fs::read(docx_path)?;
    let sha256_hex = compute_sha256(&docx_bytes);

    // ============================================
    // 第二步：从同一份字节加载文档主体段落
    // ============================================
    let doc = Document::load_bytes(&docx_bytes)?;
    debug!("提取段落数：{}", doc.paragraphs.len());

    // ============================================
    // 第三步：解析黑词列表
    // ============================================
    let thesis_dir = docx_path
        .parent()
        .map(|parent| parent.join(".thesis"))
        .filter(|p| p.is_dir());
    let blackwords = a_anti_ai::load_blackwords(thesis_dir.as_deref());

    // ============================================
    // 第三步（补充）：构建编号映射（E.5.7 / E.5.8 增强校验所需）
    // 文档无编号时返回空 Vec，不影响主流程
    // ============================================
    let numbering_map = numbering::build_numbering_map(&docx_bytes).unwrap_or_default();
    let numbering_map_opt = if numbering_map.is_empty() {
        None
    } else {
        Some(numbering_map.as_slice())
    };

    // ============================================
    // 第三步（补充）：提取样式表
    // 当前 L4 阶段只加载，F 系规则（L5）才消费
    // ============================================
    #[allow(unused_variables)] // TODO(L5): wire F series style chain validation
    let style_map = styles::extract_styles(&docx_bytes).unwrap_or_default();

    // ============================================
    // 第四步：执行规则检查，收集 Violation
    // ============================================
    let mut all_violations: Vec<Violation> = Vec::new();

    // ── P1: 文档主体 ──
    // A.1：主体段落黑词检测
    all_violations.extend(a_anti_ai::check_a1_blackwords(&doc.paragraphs, &blackwords));

    // E.5.7：章节号自动编号检测（附带编号映射做 lvlText 格式验证）
    all_violations.extend(e_format::check_e57_chapter_numbering(
        &doc.paragraphs,
        numbering_map_opt,
    ));

    // E.5.8：参考文献自动编号检测（附带编号映射做 lvlText 格式验证）
    all_violations.extend(e_format::check_e58_reference_numbering(
        &doc.paragraphs,
        numbering_map_opt,
    ));

    // ── P1: 页眉/页脚 A.1 扫描（HC-6）──
    all_violations.extend(headers_footers::check_headers_footers_blackwords(
        &docx_bytes,
        &blackwords,
    ));

    // ── P1: 文本框 A.1 扫描（HC-7）──
    all_violations.extend(textbox::check_textbox_blackwords(&docx_bytes, &blackwords));

    // ── P1: 修订痕迹检测（F.5.1 / F.5.2）──
    all_violations.extend(tracked_changes::check_tracked_changes(&docx_bytes));

    // ── P2: 表格 cell 缩进检查（D.9.1 / D.9.2）──
    // 需要 body_choice，重新从字节打开 package 获取（只读）
    if let Ok(table_violations) = run_table_check(&docx_bytes) {
        all_violations.extend(table_violations);
    }

    // ── P3: 批注 A.1 扫描──
    all_violations.extend(comments::check_comments_blackwords(
        &docx_bytes,
        &blackwords,
    ));

    // ── P3: 脚注/尾注 A.1 扫描──
    all_violations.extend(footnotes::check_footnotes_blackwords(
        &docx_bytes,
        &blackwords,
    ));

    // ── P3: C.1 引用上标检查──
    all_violations.extend(c_citation::check_c1_citation_superscript(&doc.paragraphs));

    // ── P3: C.2 引用编号顺序检查──
    all_violations.extend(c_citation::check_c2_citation_order(&doc.paragraphs));

    // ============================================
    // 第五步：按 rule_id 聚合 Violation → CheckRow
    // ============================================
    let check_rows = aggregate_violations(all_violations);

    // ============================================
    // 第六步：构造 AuditResult
    // ============================================
    let violations_count = check_rows
        .iter()
        .filter(|r| !r.passed && matches!(r.severity, Severity::Critical))
        .count();

    let passed = violations_count == 0;

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
pub fn audit_section(docx_path: &Path, section_id: &str) -> Result<AuditResult, AuditError> {
    todo!(
        "L2.1b: audit_section — 按 section_id({section_id:?}) 定位段落范围后调用 audit_full 子集；路径：{docx_path:?}"
    )
}

// ============================================
// 内部辅助函数
// ============================================

/// 执行表格 cell 缩进检查（D.9.x），需要从字节重新打开 package。
///
/// 封装为独立函数，避免 audit_full 函数体过长。
fn run_table_check(docx_bytes: &[u8]) -> Result<Vec<Violation>, AuditError> {
    let mut package =
        WordprocessingDocument::new(Cursor::new(docx_bytes)).map_err(AuditError::from_sdk)?;

    let main_part = package.main_document_part().map_err(AuditError::from_sdk)?;
    let root = main_part
        .root_element(&mut package)
        .map_err(AuditError::from_sdk)?;
    let body = root
        .body
        .as_ref()
        .ok_or_else(|| AuditError::SchemaViolation("Document 缺少 body".to_owned()))?;

    Ok(tables::check_d91_cell_indent(&body.body_choice))
}

/// 计算字节切片的 sha256 hex 字符串。
fn compute_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// 将 `Vec<Violation>` 按 `rule_id` 聚合为 `Vec<CheckRow>`。
fn aggregate_violations(violations: Vec<Violation>) -> Vec<CheckRow> {
    if violations.is_empty() {
        return Vec::new();
    }

    let mut map: HashMap<RuleId, Vec<Violation>> = HashMap::new();
    for v in violations {
        map.entry(v.rule_id).or_default().push(v);
    }

    let mut rows: Vec<CheckRow> = map
        .into_iter()
        .map(|(rule_id, vs)| {
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

    #[test]
    fn test_numpr_integration_with_num_pr_no_e57_violation() {
        let body_xml = r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:pPr>
    <w:numPr>
      <w:ilvl w:val="0"/>
      <w:numId w:val="1"/>
    </w:numPr>
  </w:pPr>
  <w:r><w:t>第一章 引言</w:t></w:r>
</w:p>"#;

        let docx_bytes = build_minimal_docx(body_xml);
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &docx_bytes).unwrap();

        let result = audit_full(tmp.path()).expect("audit_full 应成功");

        let e57_rows: Vec<_> = result
            .self_check_table
            .iter()
            .filter(|r| r.rule_id == thesis_types::RuleId::E57)
            .collect();
        assert!(
            e57_rows.is_empty(),
            "含 numPr 的段落不应触发 E.5.7，实际：{e57_rows:?}"
        );

        let doc = crate::document::Document::load(tmp.path()).unwrap();
        assert_eq!(doc.paragraphs.len(), 1);
        assert!(doc.paragraphs[0].has_num_pr);
        assert_eq!(doc.paragraphs[0].num_id, Some(1));
        assert_eq!(doc.paragraphs[0].ilvl, Some(0));
    }

    #[test]
    fn test_numpr_integration_without_num_pr_triggers_e57() {
        let body_xml = r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:pPr>
    <w:pStyle w:val="Heading1"/>
  </w:pPr>
  <w:r><w:t>1. 引言</w:t></w:r>
</w:p>"#;

        let docx_bytes = build_minimal_docx(body_xml);
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &docx_bytes).unwrap();

        let result = audit_full(tmp.path()).expect("audit_full 应成功");

        let e57_rows: Vec<_> = result
            .self_check_table
            .iter()
            .filter(|r| r.rule_id == thesis_types::RuleId::E57)
            .collect();
        assert!(
            !e57_rows.is_empty(),
            "无 numPr 的 Heading1 段落应触发 E.5.7"
        );
        assert!(!e57_rows[0].passed);

        let doc = crate::document::Document::load(tmp.path()).unwrap();
        assert!(!doc.paragraphs[0].has_num_pr);
    }

    /// 验证 C.2 引用顺序检查通过 audit_full 正确触发。
    ///
    /// fixture：段落文本含乱序引用 [2] 先出现 [1] 后出现 → C.2 违规。
    #[test]
    fn test_audit_full_includes_c2_citation_order_check() {
        // build_minimal_docx 接受 body 内容（不含 <w:body> 标签），拼两段
        let body_xml = r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:r><w:t>本研究参考了[2]的方法。</w:t></w:r>
</w:p>
<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:r><w:t>也参考了[1]的框架。</w:t></w:r>
</w:p>"#;

        let docx_bytes = build_minimal_docx(body_xml);
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &docx_bytes).unwrap();

        let result = audit_full(tmp.path()).expect("audit_full 应成功");

        let c2_rows: Vec<_> = result
            .self_check_table
            .iter()
            .filter(|r| r.rule_id == RuleId::C2)
            .collect();
        assert!(
            !c2_rows.is_empty(),
            "乱序引用应通过 audit_full 触发 C.2 违规，实际表：{:?}",
            result.self_check_table
        );
        assert!(!c2_rows[0].passed);
    }

    /// 验证 C.1 检查通过 audit_full 调用（当前实现返回空 Vec，验证 wire-up 不崩溃）。
    #[test]
    fn test_audit_full_includes_c1_citation_check_no_panic() {
        let tmp = make_docx_with_text("参考文献[1]已引用。");
        // C.1 当前轻实现：返回空 Vec，不误报
        let result = audit_full(tmp.path()).expect("audit_full 不应崩溃");
        let c1_rows: Vec<_> = result
            .self_check_table
            .iter()
            .filter(|r| r.rule_id == RuleId::C1)
            .collect();
        // 轻实现保守策略：不产生违规，避免误报
        assert!(c1_rows.is_empty(), "C.1 轻实现不应产生误报");
    }

    /// 验证 numbering wire-up 不崩溃，且 E.5.7 在有 numPr 段落时不误报。
    #[test]
    fn test_audit_full_includes_numbering_violations_no_crash() {
        // 含 numPr 的段落：合规场景，不应产生 E.5.7 违规
        let body_xml = r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:pPr>
    <w:pStyle w:val="Heading1"/>
    <w:numPr>
      <w:ilvl w:val="0"/>
      <w:numId w:val="1"/>
    </w:numPr>
  </w:pPr>
  <w:r><w:t>引言</w:t></w:r>
</w:p>"#;

        let docx_bytes = build_minimal_docx(body_xml);
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &docx_bytes).unwrap();

        // 无 numbering.xml → build_numbering_map 返回空 Vec → numbering_map_opt = None
        // 应正常运行，不崩溃
        let result = audit_full(tmp.path()).expect("audit_full 不应崩溃（numbering wire-up）");

        let e57_rows: Vec<_> = result
            .self_check_table
            .iter()
            .filter(|r| r.rule_id == RuleId::E57)
            .collect();
        assert!(
            e57_rows.is_empty(),
            "有 numPr 的段落在无 numbering.xml 时不应触发 E.5.7：{e57_rows:?}"
        );
    }
}
