//! @file tools/write_section.rs
//! @description `write_section` 工具：将新章节写入 docx，通过全量审计门控后原子落盘
//!
//! ## 流程
//! 1. 参数校验（路径、heading_level 范围）
//! 2. 读取原始 docx 字节，通过 ooxmlsdk 加载文档
//! 3. 在 body 末尾（sectPr 之前）追加 heading 段落 + 正文段落 + 参考文献段落
//! 4. 用 ooxmlsdk `save_package` 将修改后文档写入 `.docx.tmp.<nonce>` 临时文件
//! 5. 对临时文件跑 `thesis_audit::audit_full`
//!    - FAIL → 删除临时文件，返回错误（原文件不受影响）
//!    - PASS → 构造 Manifest，追加 audit-log，原子 rename 覆盖目标
//!
//! @author Atlas.oi
//! @date 2026-05-17

use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use ooxmlsdk::parts::wordprocessing_document::WordprocessingDocument;
use ooxmlsdk::schemas::schemas_openxmlformats_org_wordprocessingml_2006_main::Run as DocRun;
use ooxmlsdk::schemas::schemas_openxmlformats_org_wordprocessingml_2006_main::{
    BodyChoice, Paragraph, ParagraphChoice, ParagraphProperties, ParagraphStyleId, RunChoice, Text,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thesis_manifest::ManifestExt;
use thesis_manifest::store::AuditLog;
use thesis_types::{Manifest, RuleId, WriteOp};

// ─── 工具参数类型 ──────────────────────────────────────────────────────────────

/// 图表描述（L3.1 阶段 stub，L4 完整实现插图写入）。
#[derive(Debug, Deserialize, JsonSchema)]
#[allow(dead_code)] // L3.1 stub：path / caption 在 L4 写入图表时启用
pub struct FigureSpec {
    /// 图表文件路径（L3.1 暂不处理，占位）
    pub path: String,
    /// 图表说明文字
    pub caption: String,
}

/// 一个章节的内容描述。
#[derive(Debug, Deserialize, JsonSchema)]
#[allow(dead_code)] // figures 字段在 L4 阶段启用；style_spec 为占位
pub struct SectionSpec {
    /// 章节标题文字
    pub title: String,
    /// 标题层级（1-9），对应 Heading1..Heading9 样式
    pub heading_level: u8,
    /// 正文段落列表（每个字符串 = 一个段落）
    pub paragraphs: Vec<String>,
    /// 图表列表（L3.1 阶段忽略，不会写入 docx）
    pub figures: Vec<FigureSpec>,
    /// 参考文献列表（每条单独写一个段落）
    pub references: Vec<String>,
}

/// `write_section` 工具输入参数。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteSectionParams {
    /// 目标 docx 文件的绝对路径
    pub docx_path: String,
    /// 章节内容描述（JSON 对象）
    pub section_spec: SectionSpec,
    /// 样式规范（可选，当前 L3.1 暂未使用，占位供 L4 扩展）
    #[allow(dead_code)]
    pub style_spec: Option<serde_json::Value>,
}

/// `write_section` 工具输出。
#[derive(Debug, Serialize)]
pub struct WriteSectionOutput {
    /// 操作是否成功
    pub success: bool,
    /// 审计是否通过
    pub audit_passed: bool,
    /// 违规条数（仅统计 Critical）
    pub violations_count: usize,
    /// manifest nonce（唯一标识本次写入）
    pub nonce: String,
    /// 写入后 docx 的 sha256
    pub sha256_hex: String,
}

// ─── 核心函数 ─────────────────────────────────────────────────────────────────

/// 执行 write_section 的全部流程。
///
/// 业务逻辑：
/// 1. 校验参数
/// 2. 加载 docx，追加章节内容
/// 3. 写临时文件
/// 4. 审计临时文件
/// 5. PASS → manifest + atomic rename；FAIL → 删临时文件 + 返回错误
#[allow(clippy::too_many_lines)] // 业务流程步骤多，拆分反而降低可读性
pub fn run_write_section(params: &WriteSectionParams) -> Result<WriteSectionOutput> {
    let docx_path = PathBuf::from(&params.docx_path);

    // ────────────────────────────────────────────────────────────────
    // 步骤 1：参数校验
    // ────────────────────────────────────────────────────────────────
    if !docx_path.is_absolute() {
        anyhow::bail!("docx_path 必须是绝对路径: {}", params.docx_path);
    }
    if !docx_path.exists() {
        anyhow::bail!("docx 文件不存在: {}", docx_path.display());
    }
    let spec = &params.section_spec;
    if spec.heading_level < 1 || spec.heading_level > 9 {
        anyhow::bail!(
            "heading_level 必须在 1-9 之间，当前值: {}",
            spec.heading_level
        );
    }
    if spec.title.is_empty() {
        anyhow::bail!("section_spec.title 不能为空");
    }

    tracing::info!(
        "write_section: 开始写入章节 {:?}，heading_level={}",
        spec.title,
        spec.heading_level
    );

    // ────────────────────────────────────────────────────────────────
    // 步骤 2：读取原始 docx 字节，加载文档
    // ────────────────────────────────────────────────────────────────
    let original_bytes = std::fs::read(&docx_path)
        .with_context(|| format!("无法读取 docx: {}", docx_path.display()))?;

    let mut package = WordprocessingDocument::new(Cursor::new(original_bytes.clone()))
        .map_err(|e| anyhow::anyhow!("ooxmlsdk 加载 docx 失败: {e}"))?;

    // 取主文档 part，获取对 body 的可变引用
    let main_part = package
        .main_document_part()
        .map_err(|e| anyhow::anyhow!("取 main_document_part 失败: {e}"))?;

    let root = main_part
        .root_element_mut(&mut package)
        .map_err(|e| anyhow::anyhow!("取 root_element_mut 失败: {e}"))?;

    let body = root
        .body
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("Document 缺少 body"))?;

    // ────────────────────────────────────────────────────────────────
    // 步骤 3：构建并追加章节段落
    //
    // 追加顺序：
    //   a) 标题段落（heading_level 决定 pStyle = "Heading1"/"Heading2" 等）
    //   b) 正文段落（每条 spec.paragraphs[i]）
    //   c) 参考文献段落（每条 spec.references[i]）
    //   注：figures 在 L3.1 阶段暂不写入
    // ────────────────────────────────────────────────────────────────

    // 标题段落：使用 Heading{N} 样式
    let heading_style = format!("Heading{}", spec.heading_level);
    let heading_para = build_paragraph(Some(&heading_style), &spec.title);
    body.body_choice
        .push(BodyChoice::WP(Box::new(heading_para)));

    // 正文段落
    for text in &spec.paragraphs {
        let para = build_paragraph(None, text);
        body.body_choice.push(BodyChoice::WP(Box::new(para)));
    }

    // 参考文献段落（L3.1 简单处理：Normal 样式，每条一段）
    for reference in &spec.references {
        let para = build_paragraph(None, reference);
        body.body_choice.push(BodyChoice::WP(Box::new(para)));
    }

    tracing::debug!(
        "write_section: 新增段落 1(heading) + {} + {} 条",
        spec.paragraphs.len(),
        spec.references.len()
    );

    // ────────────────────────────────────────────────────────────────
    // 步骤 4：写入临时文件（同目录，保证 rename 原子性）
    // ────────────────────────────────────────────────────────────────
    let tmp_path = make_tmp_path(&docx_path);
    {
        // save_package 需要 Write + Seek，用 File + BufWriter
        let tmp_file = std::fs::File::create(&tmp_path)
            .with_context(|| format!("无法创建临时文件: {}", tmp_path.display()))?;
        let mut buf = std::io::BufWriter::new(tmp_file);
        ooxmlsdk::parts::save_package(&package, &mut buf)
            .map_err(|e| anyhow::anyhow!("save_package 失败: {e}"))?;
    }

    tracing::debug!("write_section: 临时文件已写入 {:?}", tmp_path);

    // ────────────────────────────────────────────────────────────────
    // 步骤 5：对临时文件执行全量审计
    // ────────────────────────────────────────────────────────────────
    let audit_result = thesis_audit::audit_full(&tmp_path)
        .with_context(|| format!("audit_full 执行失败: {}", tmp_path.display()))?;

    if !audit_result.passed {
        // 审计未通过 → 删临时文件，返回错误
        let _ = std::fs::remove_file(&tmp_path);
        tracing::warn!(
            "write_section: 审计未通过 violations={} 临时文件已删除",
            audit_result.violations_count
        );
        anyhow::bail!(
            "审计未通过：{} 条 Critical 违规；原文件未修改",
            audit_result.violations_count
        );
    }

    // ────────────────────────────────────────────────────────────────
    // 步骤 6：构造 Manifest 并追加 audit-log
    // ────────────────────────────────────────────────────────────────
    let thesis_dir = resolve_thesis_dir(&docx_path);
    std::fs::create_dir_all(&thesis_dir)?;

    // 计算 rule_hits（违规规则 → 命中次数）
    let rule_hits: HashMap<RuleId, usize> = aggregate_rule_hits(&audit_result);

    // TODO(L4): hook 层应通过环境变量或 stdin 上下文注入这两个值；
    // 当前空字符串默认导致 L4 之前生成的 manifest 无法关联到具体会话和轮次。
    let session_id = std::env::var("CLAUDE_SESSION_ID").unwrap_or_default();
    let turn_id = std::env::var("CLAUDE_TURN_ID").unwrap_or_default();

    // Manifest::new 读 tmp_path（审计后的文件）获取 sha256 + mtime
    let manifest = Manifest::new(
        tmp_path.clone(),
        WriteOp::WriteSection,
        rule_hits,
        audit_result.audit_version.clone(),
        session_id,
        turn_id,
    )
    .with_context(|| "构造 Manifest 失败")?;

    let audit_log = AuditLog::new(thesis_dir);
    audit_log
        .append(&manifest)
        .with_context(|| "写入 audit-log 失败")?;

    // ────────────────────────────────────────────────────────────────
    // 步骤 7：原子 rename 临时文件 → 目标 docx
    // ────────────────────────────────────────────────────────────────
    std::fs::rename(&tmp_path, &docx_path).with_context(|| {
        format!(
            "rename {} → {} 失败",
            tmp_path.display(),
            docx_path.display()
        )
    })?;

    tracing::info!(
        "write_section: 完成 sha256={} nonce={}",
        manifest.sha256_hex,
        manifest.nonce
    );

    Ok(WriteSectionOutput {
        success: true,
        audit_passed: true,
        violations_count: audit_result.violations_count,
        nonce: manifest.nonce.to_string(),
        sha256_hex: manifest.sha256_hex.clone(),
    })
}

// ─── 内部辅助函数 ─────────────────────────────────────────────────────────────

/// 构造一个包含单个 run 的段落。
///
/// - `style_id`：pStyle 值（如 "Heading1"），为 None 时使用默认正文样式
/// - `text`：段落文字内容
fn build_paragraph(style_id: Option<&str>, text: &str) -> Paragraph {
    // 构造 run
    let t = Text {
        xml_other_attrs: vec![],
        // 保留首尾空格（避免 Word 截断）
        space: Some(ooxmlsdk::schemas::xml::SpaceProcessingModeValues::Preserve),
        xml_content: Some(text.to_owned()),
    };
    let run = DocRun {
        xmlns: vec![],
        xml_other_attrs: vec![],
        rsid_run_properties: None,
        rsid_run_deletion: None,
        rsid_run_addition: None,
        run_properties: None,
        run_choice: vec![RunChoice::WT(Box::new(t))],
    };

    // 构造 ParagraphProperties（可选：带样式 ID）
    let paragraph_properties = style_id.map(|sid| {
        Box::new(ParagraphProperties {
            xmlns: vec![],
            xml_other_attrs: vec![],
            xml_other_children: vec![],
            paragraph_style_id: Some(ParagraphStyleId {
                val: sid.to_owned(),
            }),
            ..Default::default()
        })
    });

    Paragraph {
        xmlns: vec![],
        xml_other_attrs: vec![],
        rsid_paragraph_mark_revision: None,
        rsid_paragraph_addition: None,
        rsid_paragraph_deletion: None,
        rsid_paragraph_properties: None,
        rsid_run_addition_default: None,
        paragraph_id: None,
        text_id: None,
        no_spell_error: None,
        paragraph_properties,
        paragraph_choice: vec![ParagraphChoice::WR(Box::new(run))],
    }
}

/// 从 docx 路径推断 `.thesis/` 目录位置。
///
/// 规则：优先使用 docx 所在目录的 `.thesis/` 子目录。
fn resolve_thesis_dir(docx_path: &Path) -> PathBuf {
    docx_path.parent().unwrap_or(Path::new(".")).join(".thesis")
}

/// 为临时文件生成与 docx 同目录的唯一路径（保证 rename 原子性）。
fn make_tmp_path(docx_path: &Path) -> PathBuf {
    // 用时间戳 + 随机数作为 nonce，避免并发冲突
    let ts = Utc::now().timestamp_nanos_opt().unwrap_or(0);
    // ts 可能为负（极少情况），cast_unsigned() 是 clippy 推荐的有意转换写法
    let nonce = ts.cast_unsigned() ^ u64::from(std::process::id());
    let tmp_name = format!(
        "{}.tmp.{nonce:016x}",
        docx_path.file_name().unwrap_or_default().to_string_lossy()
    );
    docx_path.parent().unwrap_or(Path::new(".")).join(tmp_name)
}

/// 从 AuditResult 聚合 rule_hits 映射（rule_id → 命中次数）。
pub(crate) fn aggregate_rule_hits(
    audit_result: &thesis_types::AuditResult,
) -> HashMap<RuleId, usize> {
    audit_result
        .self_check_table
        .iter()
        .filter(|row| !row.passed)
        .map(|row| (row.rule_id, row.locations.len().max(1)))
        .collect()
}

// ─── 单元测试 ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    /// 复制 thesis-audit 的 build_minimal_docx（测试用，不依赖其 crate 内部函数）。
    fn build_minimal_docx(body_xml: &str) -> Vec<u8> {
        use zip::write::SimpleFileOptions;

        let document_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:wpc="http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas"
  xmlns:cx="http://schemas.microsoft.com/office/drawing/2014/chartex"
  xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"
  xmlns:o="urn:schemas-microsoft-com:office:office"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
  xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
  xmlns:v="urn:schemas-microsoft-com:vml"
  xmlns:w10="urn:schemas-microsoft-com:office:word"
  xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml"
  mc:Ignorable="w14">
  <w:body>
    {body_xml}
    <w:sectPr/>
  </w:body>
</w:document>"#
        );

        let rels_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"
    Target="word/document.xml"/>
</Relationships>"#;

        let word_rels_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
</Relationships>"#;

        let content_types_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml"
    ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;

        let buf = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(buf);
        let opts = SimpleFileOptions::default();

        zip.start_file("_rels/.rels", opts).unwrap();
        zip.write_all(rels_xml.as_bytes()).unwrap();

        zip.start_file("[Content_Types].xml", opts).unwrap();
        zip.write_all(content_types_xml.as_bytes()).unwrap();

        zip.start_file("word/document.xml", opts).unwrap();
        zip.write_all(document_xml.as_bytes()).unwrap();

        zip.start_file("word/_rels/document.xml.rels", opts)
            .unwrap();
        zip.write_all(word_rels_xml.as_bytes()).unwrap();

        zip.finish().unwrap().into_inner()
    }

    fn make_clean_docx_file() -> tempfile::NamedTempFile {
        let body_xml = r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:r><w:t>已有正文段落。</w:t></w:r>
</w:p>"#;
        let bytes = build_minimal_docx(body_xml);
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &bytes).unwrap();
        tmp
    }

    fn clean_section_spec() -> SectionSpec {
        SectionSpec {
            // heading_level=6 映射到 "Heading6"，不在 E.5.7 规则的检测列表（只检测 1-5 级）
            // 且标题文字不以数字编号开头，不触发手动章节号检测
            title: "研究背景与意义".to_string(),
            heading_level: 6,
            // 正文内容：无黑词，无手动章节号，无 [N] 引用格式
            paragraphs: vec!["本研究采用对比实验验证假设。研究结论具有重要参考价值。".to_string()],
            figures: vec![],
            // 参考文献格式：不以 [数字] 开头，避免触发 E.5.8
            references: vec!["某作者. 某论文标题. 出版社, 2024.".to_string()],
        }
    }

    #[test]
    fn write_section_clean_doc_passes_audit_and_creates_manifest() {
        let tmp = make_clean_docx_file();
        let docx_path = tmp.path().to_str().unwrap().to_string();

        // 确保 .thesis/ 目录可写（在系统临时目录下，通常可写）
        let thesis_dir = tmp.path().parent().unwrap().join(".thesis");
        std::fs::create_dir_all(&thesis_dir).unwrap();

        let result = run_write_section(&WriteSectionParams {
            docx_path: docx_path.clone(),
            section_spec: clean_section_spec(),
            style_spec: None,
        });

        assert!(result.is_ok(), "干净文档应写入成功: {:?}", result.err());
        let output = result.unwrap();
        assert!(output.audit_passed);
        assert_eq!(output.violations_count, 0);
        assert!(!output.nonce.is_empty());
        assert_eq!(output.sha256_hex.len(), 64);

        // audit-log 应存在
        let log_path = thesis_dir.join("audit-log.jsonl");
        assert!(log_path.exists(), "audit-log.jsonl 应被创建");
    }

    #[test]
    fn write_section_violation_spec_fails_audit_and_does_not_modify_original() {
        let tmp = make_clean_docx_file();
        let docx_path_str = tmp.path().to_str().unwrap().to_string();

        // 记录原始文件 sha256
        let original_bytes = std::fs::read(tmp.path()).unwrap();
        let original_sha256: String = {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(&original_bytes);
            hex::encode(h.finalize())
        };

        // 使用 heading_level=1（Heading1 无 numPr）→ 触发 E.5.7 Critical 违规
        // E.5.7 是 Critical 级别，确保 audit_result.passed = false
        let bad_spec = SectionSpec {
            title: "引言".to_string(),
            heading_level: 1,
            paragraphs: vec!["本研究采用对比实验验证假设。".to_string()],
            figures: vec![],
            references: vec![],
        };

        let result = run_write_section(&WriteSectionParams {
            docx_path: docx_path_str.clone(),
            section_spec: bad_spec,
            style_spec: None,
        });

        assert!(
            result.is_err(),
            "含 Heading1(无 numPr) 的 spec 应触发 E.5.7 Critical 审计失败"
        );

        // 原文件不应被修改
        let after_bytes = std::fs::read(tmp.path()).unwrap();
        let after_sha256: String = {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(&after_bytes);
            hex::encode(h.finalize())
        };
        assert_eq!(original_sha256, after_sha256, "原文件不应被修改");

        // 临时文件不应残留（以 .tmp. 为特征）
        let parent = tmp.path().parent().unwrap();
        let tmp_files: Vec<_> = std::fs::read_dir(parent)
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(tmp_files.is_empty(), "临时文件应被清理: {tmp_files:?}");
    }

    #[test]
    fn write_section_fails_if_relative_path() {
        let result = run_write_section(&WriteSectionParams {
            docx_path: "relative/path.docx".to_string(),
            section_spec: clean_section_spec(),
            style_spec: None,
        });
        assert!(result.is_err(), "相对路径应报错");
    }

    #[test]
    fn write_section_fails_if_heading_level_out_of_range() {
        let tmp = make_clean_docx_file();
        let result = run_write_section(&WriteSectionParams {
            docx_path: tmp.path().to_str().unwrap().to_string(),
            section_spec: SectionSpec {
                heading_level: 10,
                ..clean_section_spec()
            },
            style_spec: None,
        });
        assert!(result.is_err(), "heading_level=10 应报错");
    }
}
