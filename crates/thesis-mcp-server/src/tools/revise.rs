//! @file tools/revise.rs
//! @description `revise` 工具：以跟踪修订形式编辑 docx（蓝色插入 / 删除，无 strike 残留）
//!
//! ## 流程
//! 1. 校验参数
//! 2. 备份原文件到 `<docx_dir>/.backups/<RFC3339>-<filename>.docx`
//! 3. 读取原始 zip 字节，提取 `word/document.xml`
//! 4. 对 document.xml 字符串按 EditOp 注入 `<w:ins>` / `<w:del>` XML
//! 5. 重新打包 zip → 临时文件
//! 6. 对临时文件跑 `thesis_audit::audit_full`
//!    - FAIL → 删除临时文件，返回错误（原文件 + 备份不受影响）
//!    - PASS → 构造 Manifest，追加 audit-log，原子 rename 覆盖目标
//!
//! ## 跟踪修订实现策略
//!
//! 由于 ooxmlsdk 0.6 中 `<w:ins>` / `<w:del>` 包裹 run 的路径为：
//! `ParagraphChoice::Choice(ParagraphChoice2::WIns(RunTrackChange{ run_choice: ... }))`，
//! 类型嵌套较深，且 `RunTrackChange` 的 w:date / w:author 属性需要与 ooxmlsdk 底层
//! XML 序列化器精确配合。为确保输出的 XML 语义正确、不引入意外字段，
//! 此工具改用 **原始 XML 字符串注入**方式处理 document.xml，避免 ooxmlsdk 类型层面的
//! 序列化歧义。具体流程：zip 解包 → 字符串操作 → zip 重打包。
//!
//! @author Atlas.oi
//! @date 2026-05-17

use std::collections::HashMap;
use std::io::{Read, Write as IoWrite};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thesis_manifest::ManifestExt;
use thesis_manifest::store::AuditLog;
use thesis_types::{Manifest, RuleId, WriteOp};

use crate::tools::write_section::aggregate_rule_hits;

// ─── EditOp 类型定义 ──────────────────────────────────────────────────────────

/// 修订操作类型。
///
/// - `paragraph_index`：从 0 起始，表示 body 中第几个 `<w:p>` 元素（仅计直接 `<w:p>`，不含表格内段落）
/// - `run_index`：段落内第几个 `<w:r>` 元素，从 0 起始
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "op")]
pub enum EditOp {
    /// 在指定段落索引后插入新段落文字
    Insert {
        /// 目标段落索引（新内容插入到该段落末尾作为新段落，0-indexed）
        paragraph_index: usize,
        /// 要插入的文字内容
        text: String,
    },
    /// 删除指定段落的指定 run 区间
    Delete {
        /// 目标段落索引（0-indexed）
        paragraph_index: usize,
        /// 要删除的 run 索引区间 [start, end)（半开区间）
        run_index_range: (usize, usize),
    },
    /// 替换指定段落内某个 run 的文字
    Replace {
        /// 目标段落索引（0-indexed）
        paragraph_index: usize,
        /// 目标 run 索引（0-indexed）
        run_index: usize,
        /// 替换后的新文字
        new_text: String,
    },
    /// 改变指定 run 的格式属性
    FormatChange {
        /// 目标段落索引（0-indexed）
        paragraph_index: usize,
        /// 目标 run 索引（0-indexed）
        run_index: usize,
        /// 要修改的属性名称（如 "bold"、"italic"）
        property: String,
        /// 属性值（如 "true"、"false"）
        value: String,
    },
}

/// `revise` 工具输入参数。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReviseParams {
    /// 目标 docx 文件的绝对路径
    pub docx_path: String,
    /// 修订操作列表
    pub edits: Vec<EditOp>,
    /// 插入文字的颜色（RGB hex，不含 #）；默认蓝色 "0000FF"
    pub color: Option<String>,
}

/// `revise` 工具输出。
#[derive(Debug, Serialize)]
pub struct ReviseOutput {
    /// 操作是否成功
    pub success: bool,
    /// 审计是否通过
    pub audit_passed: bool,
    /// 违规条数（仅统计 Critical）
    pub violations_count: usize,
    /// 备份文件路径
    pub backup_path: String,
    /// manifest nonce
    pub nonce: String,
    /// 写入后 docx 的 sha256
    pub sha256_hex: String,
}

// ─── 核心函数 ─────────────────────────────────────────────────────────────────

/// 执行 revise 的全部流程。
pub fn run_revise(params: &ReviseParams) -> Result<ReviseOutput> {
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

    let color = params.color.as_deref().unwrap_or("0000FF").to_uppercase();

    // 校验颜色格式（6 位十六进制）
    if color.len() != 6 || !color.chars().all(|c| c.is_ascii_hexdigit()) {
        anyhow::bail!("color 格式无效，应为 6 位 hex（如 0000FF），当前值: {color}");
    }

    tracing::info!(
        "revise: 开始修订 {:?} edits={}",
        docx_path,
        params.edits.len()
    );

    // ────────────────────────────────────────────────────────────────
    // 步骤 2：备份原文件到 <docx_dir>/.backups/<timestamp>-<filename>.docx
    // ────────────────────────────────────────────────────────────────
    let backup_path =
        make_backup(&docx_path).with_context(|| format!("备份失败: {}", docx_path.display()))?;

    tracing::debug!("revise: 备份已创建 {:?}", backup_path);

    // ────────────────────────────────────────────────────────────────
    // 步骤 3：读取原始 zip，提取 word/document.xml
    // ────────────────────────────────────────────────────────────────
    let original_bytes = std::fs::read(&docx_path)
        .with_context(|| format!("读取 docx 失败: {}", docx_path.display()))?;

    let mut document_xml = extract_document_xml(&original_bytes)
        .with_context(|| "从 docx zip 中提取 word/document.xml 失败")?;

    // ────────────────────────────────────────────────────────────────
    // 步骤 4：按 EditOp 列表注入跟踪修订 XML
    //
    // 实现简化策略：
    //   - Insert → 在找到的第 N 个 <w:p> 结束标签后注入新 <w:p>（带 <w:ins> 包裹的 <w:r>）
    //   - Delete → 在第 N 个 <w:p> 内找第 M~K 个 <w:r>，替换为 <w:del> 包裹版本
    //   - Replace → 组合 Delete(old) + Insert(new) 在同一段落内
    //   - FormatChange → 在 run 的 <w:rPr> 追加格式属性（简单注入）
    //
    // 注意：这里的段落 / run 索引查找是基于字符串扫描，适用于结构规整的 docx。
    // 若文档有复杂嵌套（SDT、超链接内 run），偏移量可能与 body 级索引不完全对应。
    // 这是 L3.1 接受的限制（task spec 标注 figures/references 可 stub）。
    // ────────────────────────────────────────────────────────────────
    let author = "thesis-mcp";
    let date = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    for edit in &params.edits {
        document_xml = apply_edit(edit, &document_xml, &color, author, &date)
            .with_context(|| format!("应用编辑操作失败: {edit:?}"))?;
    }

    // ────────────────────────────────────────────────────────────────
    // 步骤 5：将修改后的 document.xml 重新打包为 zip，写入临时文件
    // ────────────────────────────────────────────────────────────────
    let tmp_path = make_tmp_path(&docx_path);
    repack_docx(&original_bytes, &document_xml, &tmp_path)
        .with_context(|| format!("重新打包 docx 失败 → {}", tmp_path.display()))?;

    tracing::debug!("revise: 临时文件已写入 {}", tmp_path.display());

    // ────────────────────────────────────────────────────────────────
    // 步骤 6：对临时文件执行全量审计
    // ────────────────────────────────────────────────────────────────
    let audit_result = thesis_audit::audit_full(&tmp_path)
        .with_context(|| format!("audit_full 执行失败: {}", tmp_path.display()))?;

    if !audit_result.passed {
        let _ = std::fs::remove_file(&tmp_path);
        tracing::warn!(
            "revise: 审计未通过 violations={} 临时文件已删除",
            audit_result.violations_count
        );
        anyhow::bail!(
            "{}",
            crate::tools::audit_format::format_audit_failure(&audit_result)
        );
    }

    // ────────────────────────────────────────────────────────────────
    // 步骤 7：构造 Manifest 并追加 audit-log
    // ────────────────────────────────────────────────────────────────
    let thesis_dir = resolve_thesis_dir(&docx_path);
    std::fs::create_dir_all(&thesis_dir)?;

    let rule_hits: HashMap<RuleId, usize> = aggregate_rule_hits(&audit_result);
    // TODO(L4): hook 层应通过环境变量或 stdin 上下文注入这两个值；
    // 当前空字符串默认导致 L4 之前生成的 manifest 无法关联到具体会话和轮次。
    let session_id = std::env::var("CLAUDE_SESSION_ID").unwrap_or_default();
    let turn_id = std::env::var("CLAUDE_TURN_ID").unwrap_or_default();

    let manifest = Manifest::new(
        tmp_path.clone(),
        WriteOp::Revise,
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
    // 步骤 8：原子 rename 临时文件 → 目标 docx
    // ────────────────────────────────────────────────────────────────
    std::fs::rename(&tmp_path, &docx_path).with_context(|| {
        format!(
            "rename {} → {} 失败",
            tmp_path.display(),
            docx_path.display()
        )
    })?;

    tracing::info!(
        "revise: 完成 sha256={} nonce={}",
        manifest.sha256_hex,
        manifest.nonce
    );

    Ok(ReviseOutput {
        success: true,
        audit_passed: true,
        violations_count: audit_result.violations_count,
        backup_path: backup_path.to_string_lossy().into_owned(),
        nonce: manifest.nonce.to_string(),
        sha256_hex: manifest.sha256_hex.clone(),
    })
}

// ─── 内部辅助函数 ─────────────────────────────────────────────────────────────

/// 将原文件备份到 `<docx_dir>/.backups/<RFC3339>-<basename>.docx`。
///
/// 备份目录解析策略：直接以 docx 所在目录为父目录创建 `.backups/`。
/// 不假设 docx 在 `docs/` 子目录内（若 docx 在任意路径下，备份同样紧邻源文件目录）。
fn make_backup(docx_path: &Path) -> Result<PathBuf> {
    let parent = docx_path.parent().unwrap_or(Path::new("."));
    let backup_dir = parent.join(".backups");
    std::fs::create_dir_all(&backup_dir)?;

    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let basename = docx_path.file_name().unwrap_or_default().to_string_lossy();
    let backup_name = format!("{timestamp}-{basename}");
    let backup_path = backup_dir.join(backup_name);

    std::fs::copy(docx_path, &backup_path)
        .with_context(|| format!("复制文件到备份路径失败: {}", backup_path.display()))?;

    Ok(backup_path)
}

/// 从 docx zip 字节中提取 `word/document.xml` 的 UTF-8 字符串内容。
fn extract_document_xml(zip_bytes: &[u8]) -> Result<String> {
    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor).with_context(|| "无法打开 docx zip 包")?;

    let mut file = archive
        .by_name("word/document.xml")
        .with_context(|| "docx zip 中缺少 word/document.xml")?;

    let mut xml = String::new();
    file.read_to_string(&mut xml)
        .with_context(|| "读取 word/document.xml 失败")?;

    Ok(xml)
}

/// 将修改后的 `document_xml` 重新打包为 docx zip，写入 `out_path`。
///
/// 其余 zip 条目（rels、content types 等）保持不变，仅替换 `word/document.xml`。
fn repack_docx(original_bytes: &[u8], document_xml: &str, out_path: &Path) -> Result<()> {
    let cursor = std::io::Cursor::new(original_bytes);
    let mut src = zip::ZipArchive::new(cursor).with_context(|| "无法打开原始 zip 包")?;

    let out_file = std::fs::File::create(out_path)
        .with_context(|| format!("无法创建输出文件: {}", out_path.display()))?;
    let mut writer = zip::ZipWriter::new(out_file);

    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for i in 0..src.len() {
        let mut entry = src
            .by_index(i)
            .with_context(|| format!("读取 zip 条目 {i} 失败"))?;
        let name = entry.name().to_owned();

        if name == "word/document.xml" {
            // 用修改后的 XML 替换
            writer.start_file(&name, options)?;
            writer
                .write_all(document_xml.as_bytes())
                .with_context(|| "写入修改后的 document.xml 失败")?;
        } else {
            // 其余条目直接复制
            writer.start_file(&name, options)?;
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .with_context(|| format!("读取 zip 条目 {name} 失败"))?;
            writer.write_all(&buf)?;
        }
    }

    writer.finish().with_context(|| "完成 zip 写入失败")?;

    Ok(())
}

/// 对 document.xml 字符串应用一个 EditOp，返回修改后的 XML 字符串。
///
/// 实现说明：
/// - 段落定位基于字符串中 `<w:p` 的出现序号（仅计 body 直接子段落近似）
/// - 返回的 XML 保留原有命名空间声明，不做规范化
fn apply_edit(edit: &EditOp, xml: &str, color: &str, author: &str, date: &str) -> Result<String> {
    match edit {
        EditOp::Insert {
            paragraph_index,
            text,
        } => apply_insert(xml, *paragraph_index, text, color, author, date),
        EditOp::Delete {
            paragraph_index,
            run_index_range,
        } => apply_delete(xml, *paragraph_index, *run_index_range, author, date),
        EditOp::Replace {
            paragraph_index,
            run_index,
            new_text,
        } => {
            // Replace = Delete 原 run + 在同位置插入新 run（均以跟踪修订包裹）
            let xml = apply_delete(
                xml,
                *paragraph_index,
                (*run_index, *run_index + 1),
                author,
                date,
            )?;
            // 插入位置：在该段落的 </w:p> 前（追加到段落末尾作为新插入 run）
            apply_insert_run_in_paragraph(&xml, *paragraph_index, new_text, color, author, date)
        }
        EditOp::FormatChange {
            paragraph_index,
            run_index,
            property,
            value,
        } => apply_format_change(
            xml,
            *paragraph_index,
            *run_index,
            property,
            value,
            color,
            author,
            date,
        ),
    }
}

/// 在指定段落后插入一个新段落（包含 `<w:ins>` 跟踪修订包裹的 run）。
fn apply_insert(
    xml: &str,
    paragraph_index: usize,
    text: &str,
    color: &str,
    author: &str,
    date: &str,
) -> Result<String> {
    // 定位第 paragraph_index 个 </w:p> 结束标签，在其后插入新段落
    let close_tag = "</w:p>";
    let insertion_pos = find_nth_occurrence(xml, close_tag, paragraph_index).ok_or_else(|| {
        anyhow::anyhow!("找不到第 {paragraph_index} 个段落（</w:p>），文档段落数可能不足")
    })? + close_tag.len();

    // 构造新段落：用 <w:ins> 包裹 <w:r>（无 strike，颜色由 rPr 指定）
    let new_para = build_ins_paragraph(text, color, author, date);

    let mut result = xml.to_owned();
    result.insert_str(insertion_pos, &new_para);
    Ok(result)
}

/// 在指定段落内追加一个 `<w:ins>` 包裹的 run（不创建新段落）。
fn apply_insert_run_in_paragraph(
    xml: &str,
    paragraph_index: usize,
    text: &str,
    color: &str,
    author: &str,
    date: &str,
) -> Result<String> {
    let close_tag = "</w:p>";
    let para_close_pos = find_nth_occurrence(xml, close_tag, paragraph_index)
        .ok_or_else(|| anyhow::anyhow!("找不到第 {paragraph_index} 个段落（</w:p>）"))?;

    let ins_run = build_ins_run(text, color, author, date);
    let mut result = xml.to_owned();
    result.insert_str(para_close_pos, &ins_run);
    Ok(result)
}

/// 将指定段落的指定 run 区间包裹为 `<w:del>` 跟踪删除。
///
/// 策略：找到第 paragraph_index 个 `<w:p`，在其内容区间内
/// 定位第 run_index_range.0..run_index_range.1 个 `<w:r`，
/// 将这些 `<w:r>...</w:r>` 替换为 `<w:del>...<w:r>...</w:r>...</w:del>`。
fn apply_delete(
    xml: &str,
    paragraph_index: usize,
    run_index_range: (usize, usize),
    author: &str,
    date: &str,
) -> Result<String> {
    let (para_start, para_end) = find_paragraph_range(xml, paragraph_index)
        .ok_or_else(|| anyhow::anyhow!("找不到第 {paragraph_index} 个段落"))?;

    let para_content = &xml[para_start..para_end];

    // 收集段落内所有 <w:r> 的位置区间
    let run_ranges = find_all_runs(para_content);

    let (start_idx, end_idx) = run_index_range;
    if end_idx > run_ranges.len() {
        anyhow::bail!(
            "run_index_range ({start_idx}, {end_idx}) 超出段落 {paragraph_index} 的 run 数量 {}",
            run_ranges.len()
        );
    }
    if start_idx >= end_idx {
        anyhow::bail!("run_index_range ({start_idx}, {end_idx}) 无效：start 应小于 end");
    }

    // 将 [start_idx, end_idx) 的 run 合并后包裹进 <w:del>
    let del_start = run_ranges[start_idx].0;
    let del_end = run_ranges[end_idx - 1].1;
    let runs_xml = &para_content[del_start..del_end];

    // 把原 <w:r> 内的 <w:t> 改为 <w:delText>（W3C OOXML 要求）
    let runs_as_del = runs_xml
        .replace("<w:t ", "<w:delText ")
        .replace("<w:t>", "<w:delText>")
        .replace("</w:t>", "</w:delText>");

    let del_wrapper =
        format!(r#"<w:del w:id="1" w:author="{author}" w:date="{date}">{runs_as_del}</w:del>"#);

    // 重组段落内容
    let new_para_content = format!(
        "{}{}{}",
        &para_content[..del_start],
        del_wrapper,
        &para_content[del_end..]
    );

    // 替换原文档中该段落的内容
    let mut result = xml.to_owned();
    result.replace_range(para_start..para_end, &new_para_content);
    Ok(result)
}

/// 对指定 run 的格式做变更（`<w:rPr>` 属性修改 + 跟踪格式变更）。
///
/// ## L3.1 延期存根（deferred stub）
///
/// 真正的实现需要：
/// 1. 定位目标段落（`paragraph_index`）和 run（`run_index`）
/// 2. 读取 run 的 `<w:rPr>...</w:rPr>`，修改对应属性元素
///    （例如 `property="bold"` → 插入或移除 `<w:b/>`；`property="color"` → 修改 `<w:color w:val="..."/>`）
/// 3. 将格式变更用 `<w:rPr><w:rPrChange w:id="..." w:author="..." w:date="...">旧 rPr</w:rPrChange></w:rPr>` 包裹，
///    以符合 OOXML 跟踪格式修订语义
///
/// TODO(L4.x): 实现完整 FormatChange 语义，支持 bold/italic/color/font 等属性的跟踪式格式修改。
///
/// ## 当前行为
///
/// 直接返回错误，让调用方明确知道该操作尚未实现，避免静默写入无意义内容。
#[allow(clippy::too_many_arguments)]
fn apply_format_change(
    _xml: &str,
    paragraph_index: usize,
    run_index: usize,
    property: &str,
    value: &str,
    _color: &str,
    _author: &str,
    _date: &str,
) -> Result<String> {
    anyhow::bail!(
        "FormatChange 尚未实现（L4.x 计划中）：段落 {paragraph_index} 第 {run_index} 个 run，\
        属性 {property}={value}；请改用 Insert/Delete/Replace"
    )
}

// ─── XML 构造辅助 ──────────────────────────────────────────────────────────────

/// 构造一个用 `<w:ins>` 包裹 `<w:r>` 的新段落 XML 字符串。
fn build_ins_paragraph(text: &str, color: &str, author: &str, date: &str) -> String {
    format!(
        r#"<w:p><w:ins w:id="1" w:author="{author}" w:date="{date}">{}</w:ins></w:p>"#,
        build_colored_run_xml(text, color)
    )
}

/// 构造一个用 `<w:ins>` 包裹 `<w:r>` 的 run XML 字符串（不含外层 `<w:p>`）。
fn build_ins_run(text: &str, color: &str, author: &str, date: &str) -> String {
    format!(
        r#"<w:ins w:id="1" w:author="{author}" w:date="{date}">{}</w:ins>"#,
        build_colored_run_xml(text, color)
    )
}

/// 构造带颜色的 `<w:r>` XML，不含 strike。
///
/// `<w:color w:val="RRGGBB"/>` 用于视觉区分插入内容。
fn build_colored_run_xml(text: &str, color: &str) -> String {
    let escaped = xml_escape(text);
    format!(
        r#"<w:r><w:rPr><w:color w:val="{color}"/></w:rPr><w:t xml:space="preserve">{escaped}</w:t></w:r>"#
    )
}

/// 对文本做 XML 最小转义（仅处理 & < > " '）。
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// ─── 字符串搜索辅助 ────────────────────────────────────────────────────────────

/// 查找字符串 `haystack` 中第 `n`（0-indexed）次出现 `needle` 的字节偏移位置。
fn find_nth_occurrence(haystack: &str, needle: &str, n: usize) -> Option<usize> {
    let mut count = 0;
    let mut search_from = 0;

    loop {
        match haystack[search_from..].find(needle) {
            Some(rel_pos) => {
                let abs_pos = search_from + rel_pos;
                if count == n {
                    return Some(abs_pos);
                }
                count += 1;
                search_from = abs_pos + needle.len();
            }
            None => return None,
        }
    }
}

/// 定位 document.xml 中第 `n` 个（0-indexed）`<w:p` ... `</w:p>` 的字节区间 [start, end)。
///
/// 返回的区间为段落标签内部内容（不含开头的 `<w:p...>`，含 `</w:p>`）。
/// 这里简化处理：start 为 `<w:p` 之后（到第 `>` 结束），end 为 `</w:p>` 之后。
///
/// 实际返回 `[para_open_pos, para_close_pos + "</w:p>".len())` 即整段含标签的 XML 区间。
fn find_paragraph_range(xml: &str, n: usize) -> Option<(usize, usize)> {
    let para_open = "<w:p";
    let para_close = "</w:p>";

    let mut count = 0;
    let mut search_from = 0;

    loop {
        let open_rel = xml[search_from..].find(para_open)?;
        let open_abs = search_from + open_rel;

        if count == n {
            // 找到了第 n 个段落开始，定位其结束
            let close_rel = xml[open_abs..].find(para_close)?;
            let close_abs = open_abs + close_rel + para_close.len();
            return Some((open_abs, close_abs));
        }

        count += 1;
        search_from = open_abs + para_open.len();
    }
}

/// 在段落 XML 片段中查找所有 `<w:r` ... `</w:r>` 的字节区间 (start, end)。
fn find_all_runs(para_xml: &str) -> Vec<(usize, usize)> {
    let run_open = "<w:r";
    let run_close = "</w:r>";
    let mut ranges = Vec::new();
    let mut search_from = 0;

    while let Some(open_rel) = para_xml[search_from..].find(run_open) {
        let open_abs = search_from + open_rel;

        // 确认不是 <w:rPr>（RunProperties 标签）：下一个字符不应是字母
        let after_open = open_abs + run_open.len();
        if let Some(next_ch) = para_xml[after_open..].chars().next()
            && next_ch.is_alphabetic()
        {
            // 这是 <w:rXxx> 而非 <w:r>，跳过
            search_from = open_abs + run_open.len();
            continue;
        }

        let Some(close_rel) = para_xml[open_abs..].find(run_close) else {
            break;
        };
        let close_abs = open_abs + close_rel + run_close.len();

        ranges.push((open_abs, close_abs));
        search_from = close_abs;
    }

    ranges
}

// ─── 路径辅助 ─────────────────────────────────────────────────────────────────

fn resolve_thesis_dir(docx_path: &Path) -> PathBuf {
    docx_path.parent().unwrap_or(Path::new(".")).join(".thesis")
}

fn make_tmp_path(docx_path: &Path) -> PathBuf {
    let ts = Utc::now().timestamp_nanos_opt().unwrap_or(0);
    // cast_unsigned() 将 i64（可能为负）按位转为 u64，是 clippy 推荐的有意转换写法
    let nonce = ts.cast_unsigned() ^ u64::from(std::process::id());
    let tmp_name = format!(
        "{}.tmp.{nonce:016x}",
        docx_path.file_name().unwrap_or_default().to_string_lossy()
    );
    docx_path.parent().unwrap_or(Path::new(".")).join(tmp_name)
}

// ─── 单元测试 ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    fn build_minimal_docx(body_xml: &str) -> Vec<u8> {
        use std::io::Write as _;
        use zip::write::SimpleFileOptions;

        let document_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:wpc="http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas"
  xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
  xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"
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

    fn make_clean_docx() -> tempfile::NamedTempFile {
        let body = r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:r><w:t>原有正文段落。</w:t></w:r>
</w:p>"#;
        let bytes = build_minimal_docx(body);
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &bytes).unwrap();
        tmp
    }

    /// 为测试创建 docs/.backups 目录（make_backup 内部会创建，但测试需要在 tmp 目录结构下）。
    fn setup_project_dirs(docx_path: &Path) {
        // thesis dir
        let thesis_dir = docx_path.parent().unwrap().join(".thesis");
        std::fs::create_dir_all(&thesis_dir).unwrap();
    }

    #[test]
    fn revise_insert_creates_backup_and_passes_audit() {
        let tmp = make_clean_docx();
        setup_project_dirs(tmp.path());

        let result = run_revise(&ReviseParams {
            docx_path: tmp.path().to_str().unwrap().to_string(),
            edits: vec![EditOp::Insert {
                paragraph_index: 0,
                text: "新插入的段落内容。".to_string(),
            }],
            color: Some("0000FF".to_string()),
        });

        assert!(result.is_ok(), "Insert 操作应成功: {:?}", result.err());
        let output = result.unwrap();
        assert!(output.audit_passed);

        // 备份文件应存在
        let backup_path = PathBuf::from(&output.backup_path);
        assert!(
            backup_path.exists(),
            "备份文件应存在: {}",
            output.backup_path
        );

        // 备份目录应为 docs/.backups
        let backup_dir_name = backup_path
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy();
        assert_eq!(backup_dir_name, ".backups", "备份应在 .backups 目录");
    }

    #[test]
    fn revise_insert_xml_contains_w_ins_no_strike() {
        let tmp = make_clean_docx();
        setup_project_dirs(tmp.path());

        run_revise(&ReviseParams {
            docx_path: tmp.path().to_str().unwrap().to_string(),
            edits: vec![EditOp::Insert {
                paragraph_index: 0,
                text: "测试插入。".to_string(),
            }],
            color: Some("0000FF".to_string()),
        })
        .expect("revise 应成功");

        // 读取写入后的 docx，检查 document.xml
        let bytes = std::fs::read(tmp.path()).unwrap();
        let doc_xml = extract_document_xml(&bytes).unwrap();

        assert!(doc_xml.contains("<w:ins "), "document.xml 应包含 <w:ins");
        assert!(
            !doc_xml.contains("<w:strike"),
            "document.xml 不应包含 <w:strike>"
        );
        assert!(doc_xml.contains("0000FF"), "应包含蓝色颜色属性");
    }

    #[test]
    fn revise_fails_if_relative_path() {
        let result = run_revise(&ReviseParams {
            docx_path: "relative.docx".to_string(),
            edits: vec![],
            color: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn revise_fails_if_invalid_color() {
        let tmp = make_clean_docx();
        let result = run_revise(&ReviseParams {
            docx_path: tmp.path().to_str().unwrap().to_string(),
            edits: vec![],
            color: Some("GGGGGG".to_string()),
        });
        assert!(result.is_err(), "无效颜色格式应报错");
    }

    #[test]
    fn find_nth_occurrence_basic() {
        let s = "abc</w:p>def</w:p>ghi</w:p>";
        assert_eq!(find_nth_occurrence(s, "</w:p>", 0), Some(3));
        assert_eq!(find_nth_occurrence(s, "</w:p>", 1), Some(12));
        assert_eq!(find_nth_occurrence(s, "</w:p>", 2), Some(21));
        assert_eq!(find_nth_occurrence(s, "</w:p>", 3), None);
    }

    #[test]
    fn xml_escape_handles_special_chars() {
        let result = xml_escape("a<b>&\"'");
        assert_eq!(result, "a&lt;b&gt;&amp;&quot;&apos;");
    }

    #[test]
    fn revise_insert_blue_color_matches_spec() {
        // 规范要求：蓝色 RGB = 0,0,255 → hex = 0000FF
        let run_xml = build_colored_run_xml("test", "0000FF");
        assert!(run_xml.contains(r#"w:val="0000FF""#), "颜色应为 0000FF");
        assert!(!run_xml.contains("strike"), "不应含 strike");
    }

    // ─── 辅助：构造含两个 run 的最小段落 docx ──────────────────────────────────

    fn make_two_run_docx() -> tempfile::NamedTempFile {
        // 段落 0：两个 run — "Hello " 和 "World"
        let body = r#"<w:p>
  <w:r><w:t xml:space="preserve">Hello </w:t></w:r>
  <w:r><w:t>World</w:t></w:r>
</w:p>"#;
        let bytes = build_minimal_docx(body);
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &bytes).unwrap();
        tmp
    }

    // ─── Issue 1: EditOp::Delete 单元测试（F.5.2 审计覆盖）────────────────────

    /// 验证 apply_delete 将目标 run 包裹为 `<w:del>` / `<w:delText>`，
    /// 无 `<w:strike>` 残留。
    #[test]
    fn apply_delete_wraps_run_in_w_del_no_strike() {
        // 构造含两个 run 的段落 XML（不需要完整 docx zip，直接测试字符串操作函数）
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t xml:space="preserve">Hello </w:t></w:r>
      <w:r><w:t>World</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;

        let author = "thesis-mcp";
        let date = "2026-05-18T00:00:00Z";

        // 删除第 0 个段落的第 0 个 run（"Hello "）
        let result = apply_delete(xml, 0, (0, 1), author, date).expect("apply_delete 应成功");

        // 输出 XML 应包含 <w:del>
        assert!(
            result.contains("<w:del "),
            "输出 XML 应包含 <w:del>，实际:\n{result}"
        );
        // 原 <w:t> 应转换为 <w:delText>
        assert!(
            result.contains("<w:delText"),
            "输出 XML 应包含 <w:delText>，实际:\n{result}"
        );
        // "World" run 保持原样（不受影响）
        assert!(
            result.contains("<w:t>World</w:t>"),
            "未删除的 run 应保持原样"
        );
        // 不应出现 <w:strike>
        assert!(
            !result.contains("<w:strike"),
            "输出 XML 不应包含 <w:strike>"
        );
        // "Hello " 文本应在 <w:del> 内
        assert!(
            result.contains("Hello "),
            "被删除的文本仍应出现在 <w:del> 中"
        );
    }

    /// 端到端：通过 run_revise 执行 Delete，验证写入后 docx 的 document.xml。
    #[test]
    fn revise_delete_xml_contains_w_del_no_strike() {
        let tmp = make_two_run_docx();
        setup_project_dirs(tmp.path());

        run_revise(&ReviseParams {
            docx_path: tmp.path().to_str().unwrap().to_string(),
            edits: vec![EditOp::Delete {
                paragraph_index: 0,
                run_index_range: (0, 1),
            }],
            color: Some("0000FF".to_string()),
        })
        .expect("Delete 操作应成功");

        let bytes = std::fs::read(tmp.path()).unwrap();
        let doc_xml = extract_document_xml(&bytes).unwrap();

        assert!(doc_xml.contains("<w:del "), "document.xml 应包含 <w:del>");
        assert!(
            doc_xml.contains("<w:delText"),
            "document.xml 应包含 <w:delText>"
        );
        assert!(
            !doc_xml.contains("<w:strike"),
            "document.xml 不应包含 <w:strike>"
        );
    }

    // ─── Issue 2: EditOp::Replace 单元测试（F.5.2 审计覆盖）──────────────────

    /// 验证 apply_edit Replace 同时产出 `<w:del>` 和 `<w:ins>`，无 `<w:strike>`。
    #[test]
    fn apply_replace_produces_del_and_ins_no_strike() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>Hello</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;

        let author = "thesis-mcp";
        let date = "2026-05-18T00:00:00Z";
        let color = "0000FF";

        let result = apply_edit(
            &EditOp::Replace {
                paragraph_index: 0,
                run_index: 0,
                new_text: "Goodbye".to_string(),
            },
            xml,
            color,
            author,
            date,
        )
        .expect("apply_edit Replace 应成功");

        // 应同时含 <w:del>（原文 Hello）和 <w:ins>（新文 Goodbye）
        assert!(
            result.contains("<w:del "),
            "Replace 后应包含 <w:del>，实际:\n{result}"
        );
        assert!(
            result.contains("<w:ins "),
            "Replace 后应包含 <w:ins>，实际:\n{result}"
        );
        // 原文被 del 包裹
        assert!(result.contains("Hello"), "原文 'Hello' 应出现在 <w:del> 中");
        // 新文被 ins 包裹
        assert!(
            result.contains("Goodbye"),
            "新文 'Goodbye' 应出现在 <w:ins> 中"
        );
        // 无 strike 残留
        assert!(
            !result.contains("<w:strike"),
            "Replace 后不应包含 <w:strike>"
        );
    }

    /// 端到端：通过 run_revise 执行 Replace，验证写入后 docx 的 document.xml。
    #[test]
    fn revise_replace_xml_has_del_and_ins_no_strike() {
        let tmp = make_two_run_docx();
        setup_project_dirs(tmp.path());

        run_revise(&ReviseParams {
            docx_path: tmp.path().to_str().unwrap().to_string(),
            edits: vec![EditOp::Replace {
                paragraph_index: 0,
                run_index: 0,
                new_text: "Goodbye".to_string(),
            }],
            color: Some("0000FF".to_string()),
        })
        .expect("Replace 操作应成功");

        let bytes = std::fs::read(tmp.path()).unwrap();
        let doc_xml = extract_document_xml(&bytes).unwrap();

        assert!(doc_xml.contains("<w:del "), "document.xml 应包含 <w:del>");
        assert!(doc_xml.contains("<w:ins "), "document.xml 应包含 <w:ins>");
        assert!(
            !doc_xml.contains("<w:strike"),
            "document.xml 不应包含 <w:strike>"
        );
    }

    // ─── Issue 3: FormatChange 应返回 Err ─────────────────────────────────────

    /// 验证 FormatChange 返回明确的 Err，而不是静默写入无意义内容。
    #[test]
    fn format_change_returns_err_not_implemented() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>Test</w:t></w:r></w:p>
  </w:body>
</w:document>"#;

        let result = apply_edit(
            &EditOp::FormatChange {
                paragraph_index: 0,
                run_index: 0,
                property: "bold".to_string(),
                value: "true".to_string(),
            },
            xml,
            "0000FF",
            "thesis-mcp",
            "2026-05-18T00:00:00Z",
        );

        assert!(result.is_err(), "FormatChange 应返回 Err（尚未实现）");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("FormatChange"),
            "错误信息应提及 FormatChange，实际: {msg}"
        );
    }
}
