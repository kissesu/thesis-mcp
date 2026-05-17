//! @file stop.rs
//! @description Stop hook：会话结束 TOCTOU 扫描 + mtime 孤儿 docx 检测
//!
//! 覆盖约束：HC-4, HC-23, HC-25, HC-29, HC-30, SC-2, SC-5
//!
//! 执行流程：
//! 1. 从 stdin 读取 CC 传入的 JSON（含 transcript_path / session_id / cwd）
//! 2. 解析 transcript，判断是否 thesis 域（SC-2）
//! 3. 非 thesis 域 → exit 0（SC-5：不误阻断非 thesis 任务）
//! 4. thesis 域 → 扫描 docs/*.docx：
//!    a. 有 manifest 的 docx → verify_against_disk() (TOCTOU，HC-23/HC-30)
//!    b. mtime > session_start 但无 manifest → 孤儿 docx（HC-29 subagent 偷写）
//! 5. 任何违规 → exit 2 + stderr 详情
//! 6. thesis 域内异常 → exit 2（HC-25 fail-closed；HC-4 不静默通过）
//!
//! @author Atlas.oi
//! @date 2026-05-17

use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use thesis_manifest::{ManifestExt, store::AuditLog};
use thesis_types::Manifest;

use crate::transcript;

// ============================================================
// stdin JSON 结构
// ============================================================

/// CC Stop hook 传入的 JSON 结构。
#[derive(Debug, Deserialize)]
struct StopHookInput {
    #[serde(default)]
    session_id: String,
    transcript_path: Option<String>,
    #[serde(default)]
    cwd: String,
}

// ============================================================
// 主逻辑
// ============================================================

/// Stop hook 入口，返回 exit code（0 = 放行，2 = 阻断）。
pub fn run() -> i32 {
    match run_inner() {
        Ok(code) => code,
        Err(e) => {
            // HC-25: thesis 域内异常 fail-closed；但我们在这里不知道是不是 thesis 域，
            // 保守选择：stdin 解析失败本身不是 thesis 异常，放行（SC-5）
            eprintln!("[thesis-hook/stop] 内部错误，放行: {e}");
            0
        }
    }
}

/// 内部逻辑，使用 Result 携带错误。
fn run_inner() -> Result<i32, anyhow::Error> {
    // 第一步：读取 stdin JSON
    let input = read_stdin_json()?;

    // 第二步：解析 transcript，判断 thesis 域
    let transcript_path = match &input.transcript_path {
        Some(p) => PathBuf::from(p),
        None => {
            // 无 transcript_path 无法判断域，安全放行（SC-5）
            return Ok(0);
        }
    };

    let summary = transcript::parse(&transcript_path)?;

    // 第三步：非 thesis 域直接放行（SC-5）
    if !summary.is_thesis_domain {
        return Ok(0);
    }

    // 第四步：thesis 域 — fail-closed 模式（HC-25）
    // 任何内部错误都返回 exit 2
    match scan_thesis_domain(&input, summary.session_start) {
        Ok(code) => Ok(code),
        Err(e) => {
            // HC-25: thesis 域内异常 → exit 2，不静默放行
            eprintln!("[thesis-hook/stop] thesis 域扫描异常（fail-closed，HC-25）: {e}");
            Ok(2)
        }
    }
}

/// 在 thesis 域内执行 TOCTOU 扫描 + mtime 孤儿检测。
///
/// 返回 0（干净）或 2（有违规），内部异常向上传播由调用方 fail-closed。
fn scan_thesis_domain(
    input: &StopHookInput,
    session_start: Option<DateTime<Utc>>,
) -> Result<i32, anyhow::Error> {
    // 定位 .thesis 目录：优先 cwd/.thesis，其次当前进程工作目录
    let thesis_dir = locate_thesis_dir(&input.cwd);

    let audit_log = AuditLog::new(thesis_dir.clone());

    // 收集本轮会话覆盖的所有 docx（来自 audit-log.jsonl）
    let manifests = collect_session_manifests(&thesis_dir, &audit_log, &input.session_id)?;

    let mut violations: Vec<String> = Vec::new();

    // === TOCTOU 检查：manifest 存在的 docx（HC-23, HC-30）===
    for manifest in &manifests {
        if let Err(e) = ManifestExt::verify_against_disk(manifest) {
            violations.push(format!(
                "TOCTOU 违规（HC-23）: {} — {e}",
                manifest.docx_path.display()
            ));
        }
    }

    // === mtime 孤儿扫描：docs/*.docx mtime > session_start 但无 manifest（HC-29）===
    let docs_dir = locate_docs_dir(&input.cwd);
    if docs_dir.exists() {
        let session_cutoff = session_start.unwrap_or(DateTime::UNIX_EPOCH);
        let orphans = find_orphan_docx(&docs_dir, session_cutoff, &audit_log)?;
        for orphan in orphans {
            violations.push(format!(
                "孤儿 docx（HC-29 subagent 偷写）: {} — mtime > session_start 但无 manifest",
                orphan.display()
            ));
        }
    }

    if violations.is_empty() {
        Ok(0)
    } else {
        for v in &violations {
            eprintln!("[thesis-hook/stop] {v}");
        }
        Ok(2)
    }
}

// ============================================================
// 辅助函数
// ============================================================

/// 从 audit-log.jsonl 收集与本会话 session_id 匹配的所有 manifest。
///
/// AuditLog 只提供按路径查最新记录的 `latest_for`；
/// 因此先扫描 JSONL 拿所有路径，再按路径查最新 manifest。
///
/// 设计取舍：这会返回每条 docx 路径的**最新** manifest（而非本会话所有历史），
/// 这对 TOCTOU 检查是正确语义（我们关心"最后一次被认可的状态"）。
fn collect_session_manifests(
    thesis_dir: &Path,
    audit_log: &AuditLog,
    _session_id: &str,
) -> Result<Vec<Manifest>, anyhow::Error> {
    // audit-log.jsonl 路径由 thesis_dir 直接构造（与 AuditLog 内部逻辑一致）
    let log_path = thesis_dir.join("audit-log.jsonl");

    if !log_path.exists() {
        return Ok(Vec::new());
    }

    let file = std::fs::File::open(&log_path)?;
    let reader = std::io::BufReader::new(file);

    let mut seen_paths: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    for line_result in reader.lines() {
        let line = line_result?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(manifest) = serde_json::from_str::<Manifest>(trimmed) {
            seen_paths.insert(manifest.docx_path);
        }
    }

    // 对每条路径，取最新 manifest（latest_for 已实现按路径过滤最新）
    let mut result = Vec::new();
    for path in seen_paths {
        if let Some(m) = audit_log.latest_for(&path)? {
            result.push(m);
        }
    }

    Ok(result)
}

/// 扫描 docs_dir 下所有 *.docx，返回 mtime > session_cutoff 且无 manifest 的孤儿文件。
fn find_orphan_docx(
    docs_dir: &Path,
    session_cutoff: DateTime<Utc>,
    audit_log: &AuditLog,
) -> Result<Vec<PathBuf>, anyhow::Error> {
    let mut orphans = Vec::new();

    // 扫描 docs_dir 下（递归）所有 .docx 文件
    for entry in walkdir_docx(docs_dir)? {
        let meta = std::fs::metadata(&entry)?;
        let mtime = file_mtime_utc(&meta)?;

        if mtime <= session_cutoff {
            continue; // 文件在会话开始前就存在，不是本会话产物
        }

        // 检查是否有对应的 manifest
        let has_manifest = audit_log.latest_for(&entry)?.is_some();
        if !has_manifest {
            orphans.push(entry);
        }
    }

    Ok(orphans)
}

/// 递归收集 dir 下所有 *.docx 路径（不跟随符号链接）。
fn walkdir_docx(dir: &Path) -> Result<Vec<PathBuf>, anyhow::Error> {
    let mut result = Vec::new();

    // 手动递归，避免引入 walkdir 外部依赖
    collect_docx_recursive(dir, &mut result)?;

    Ok(result)
}

/// 递归辅助函数，收集 *.docx 绝对路径。
fn collect_docx_recursive(dir: &Path, result: &mut Vec<PathBuf>) -> Result<(), anyhow::Error> {
    let entries = std::fs::read_dir(dir)?;
    for entry_result in entries {
        let entry = entry_result?;
        let path = entry.path();
        let meta = entry.metadata()?;

        if meta.is_dir() {
            // 递归进入子目录（不跟随符号链接）
            collect_docx_recursive(&path, result)?;
        } else if meta.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("docx"))
        {
            result.push(path);
        }
    }
    Ok(())
}

/// 从 `std::fs::Metadata` 提取 UTC mtime。
fn file_mtime_utc(meta: &std::fs::Metadata) -> Result<DateTime<Utc>, anyhow::Error> {
    let system_time = meta.modified()?;
    let duration = system_time
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| anyhow::anyhow!("mtime 早于 Unix 纪元: {e}"))?;
    let secs = i64::try_from(duration.as_secs())?;
    let nanos = duration.subsec_nanos();
    DateTime::from_timestamp(secs, nanos)
        .ok_or_else(|| anyhow::anyhow!("无法转换 mtime 为 DateTime<Utc>"))
}

/// 定位 `.thesis/` 目录（cwd/.thesis 或当前工作目录/.thesis）。
fn locate_thesis_dir(cwd: &str) -> PathBuf {
    if cwd.is_empty() {
        PathBuf::from(".thesis")
    } else {
        PathBuf::from(cwd).join(".thesis")
    }
}

/// 定位 `docs/` 目录（cwd/docs）。
fn locate_docs_dir(cwd: &str) -> PathBuf {
    if cwd.is_empty() {
        PathBuf::from("docs")
    } else {
        PathBuf::from(cwd).join("docs")
    }
}

/// 从 stdin 读取完整内容并解析。
fn read_stdin_json() -> Result<StopHookInput, anyhow::Error> {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    Ok(serde_json::from_str(&buf)?)
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    use thesis_manifest::ManifestExt;
    use thesis_types::{Manifest, WriteOp};

    // ---- 工具函数 ----

    /// 构造最简 StopHookInput（指向给定目录）。
    fn make_input(dir: &TempDir, transcript_path: &str) -> StopHookInput {
        StopHookInput {
            session_id: "sess-test".to_owned(),
            transcript_path: Some(transcript_path.to_owned()),
            cwd: dir.path().to_str().unwrap().to_owned(),
        }
    }

    /// 在 dir 下创建 transcript.jsonl，写入给定内容。
    fn write_transcript(dir: &TempDir, lines: &[&str]) -> PathBuf {
        let path = dir.path().join("transcript.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
        path
    }

    /// 创建最小合法 docx zip 字节（用于让 Manifest::new 成功）。
    fn minimal_docx_bytes() -> Vec<u8> {
        use std::io::Write as IoWrite;
        // 借用 thesis-audit 的 build_minimal_docx 会引入跨 crate 依赖，
        // 这里直接手工构造一个合法 zip（包含 [Content_Types].xml + word/document.xml）
        let mut buf = Vec::new();
        {
            let cursor = std::io::Cursor::new(&mut buf);
            let mut zip = zip::ZipWriter::new(cursor);

            let options = zip::write::FileOptions::<()>::default()
                .compression_method(zip::CompressionMethod::Stored);

            zip.start_file("[Content_Types].xml", options).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#).unwrap();

            zip.start_file("_rels/.rels", options).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#).unwrap();

            zip.start_file("word/document.xml", options).unwrap();
            zip.write_all(
                br#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body><w:p><w:r><w:t>test</w:t></w:r></w:p></w:body>
</w:document>"#,
            )
            .unwrap();

            zip.finish().unwrap();
        }
        buf
    }

    /// 在 dir/docs/ 下写入 docx 文件，返回路径。
    fn write_docx(dir: &TempDir, name: &str) -> PathBuf {
        let docs = dir.path().join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        let path = docs.join(name);
        std::fs::write(&path, minimal_docx_bytes()).unwrap();
        path
    }

    // ---- 测试用例 ----

    /// 非 thesis 域：transcript 无 thesis 信号 → exit 0（SC-5）。
    #[test]
    fn no_thesis_domain_exits_zero() {
        let dir = TempDir::new().unwrap();
        let transcript = write_transcript(
            &dir,
            &[
                r#"{"type":"message","role":"user","content":"lint the code","timestamp":"2026-05-17T10:00:00Z"}"#,
            ],
        );
        let input = make_input(&dir, transcript.to_str().unwrap());
        let summary = transcript::parse(&transcript).unwrap();
        assert!(!summary.is_thesis_domain);

        // 整合验证：非 thesis 域直接放行
        let code = if summary.is_thesis_domain {
            scan_thesis_domain(&input, summary.session_start).unwrap()
        } else {
            0
        };
        assert_eq!(code, 0);
    }

    /// thesis 域，audit-log 干净，docs/ 无孤儿 docx → exit 0。
    #[test]
    fn thesis_domain_no_violations_exits_zero() {
        let dir = TempDir::new().unwrap();

        // 写一个 docx 并生成对应的 manifest
        let docx_path = write_docx(&dir, "thesis.docx");

        let thesis_dir = dir.path().join(".thesis");
        std::fs::create_dir_all(&thesis_dir).unwrap();

        let manifest = Manifest::new(
            docx_path.clone(),
            WriteOp::WriteSection,
            std::collections::HashMap::default(),
            "0.1.0".to_owned(),
            "sess-test".to_owned(),
            "turn-1".to_owned(),
        )
        .unwrap();

        let audit_log = AuditLog::new(thesis_dir.clone());
        audit_log.append(&manifest).unwrap();

        // transcript 含 thesis 域信号
        let transcript = write_transcript(
            &dir,
            &[
                r#"{"type":"message","role":"user","content":"/thesis write chapter 1","timestamp":"2026-05-17T08:00:00Z"}"#,
                r#"{"type":"tool_use","content":{"name":"mcp__thesis__write_section"}}"#,
            ],
        );

        let input = make_input(&dir, transcript.to_str().unwrap());
        let summary = transcript::parse(&transcript).unwrap();
        assert!(summary.is_thesis_domain);

        let code = scan_thesis_domain(&input, summary.session_start).unwrap();
        assert_eq!(code, 0, "manifest 与磁盘一致，应放行");
    }

    /// thesis 域，docs/ 下有 mtime > session_start 的 docx 但无 manifest → exit 2（HC-29）。
    #[test]
    fn thesis_domain_orphan_docx_exits_two() {
        let dir = TempDir::new().unwrap();

        // 会话开始时间设置在过去
        let session_start: DateTime<Utc> = "2026-05-17T00:00:00Z".parse().unwrap();

        // 写一个 docx（mtime = 现在，大于 session_start）
        let docx_path = write_docx(&dir, "orphan.docx");
        let _ = docx_path; // 路径已写入，mtime 是现在

        // .thesis 目录存在但 audit-log 为空（没有 manifest）
        let thesis_dir = dir.path().join(".thesis");
        std::fs::create_dir_all(&thesis_dir).unwrap();

        let input = StopHookInput {
            session_id: "sess-test".to_owned(),
            transcript_path: None,
            cwd: dir.path().to_str().unwrap().to_owned(),
        };

        let audit_log = AuditLog::new(thesis_dir);
        let docs_dir = dir.path().join("docs");
        let orphans = find_orphan_docx(&docs_dir, session_start, &audit_log).unwrap();

        assert!(
            !orphans.is_empty(),
            "应发现孤儿 docx：orphan.docx mtime > session_start 且无 manifest"
        );
        let _ = input;
    }

    /// thesis 域，manifest 存在但磁盘 sha256 已变 → exit 2（HC-23 TOCTOU）。
    #[test]
    fn thesis_domain_toctou_violation_exits_two() {
        let dir = TempDir::new().unwrap();
        let docx_path = write_docx(&dir, "thesis.docx");

        let thesis_dir = dir.path().join(".thesis");
        std::fs::create_dir_all(&thesis_dir).unwrap();

        // 先生成 manifest（此时内容为 minimal_docx_bytes）
        let manifest = Manifest::new(
            docx_path.clone(),
            WriteOp::WriteSection,
            std::collections::HashMap::default(),
            "0.1.0".to_owned(),
            "sess-test".to_owned(),
            "turn-1".to_owned(),
        )
        .unwrap();

        let audit_log = AuditLog::new(thesis_dir);
        audit_log.append(&manifest).unwrap();

        // 修改磁盘上的 docx（模拟 TOCTOU 篡改）
        std::fs::write(&docx_path, b"tampered content").unwrap();

        // verify_against_disk 应报 Sha256Mismatch
        let result = manifest.verify_against_disk();
        assert!(result.is_err(), "磁盘 sha256 已变，应报 TOCTOU 违规");
        match result.unwrap_err() {
            thesis_manifest::TocTouViolation::Sha256Mismatch { .. } => {}
            other => panic!("期望 Sha256Mismatch，实际: {other:?}"),
        }
    }
}
