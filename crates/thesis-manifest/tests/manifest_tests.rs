//! @file manifest_tests.rs
//! @description thesis-manifest 集成测试：round-trip / TOCTOU 检测 / 并发 AuditLog
//! @author Atlas.oi
//! @date 2026-05-17

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tempfile::TempDir;
use thesis_manifest::ManifestExt;
use thesis_manifest::store::AuditLog;
use thesis_types::{Manifest, WriteOp};

// ============================================================
// 辅助函数
// ============================================================

/// 在临时目录写一个假 docx 文件，返回路径
fn write_fake_docx(dir: &TempDir, name: &str, content: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, content).unwrap();
    path
}

/// 构造最小化的 Manifest（rule_hits 为空）
fn make_manifest(docx_path: PathBuf) -> Manifest {
    Manifest::new(
        docx_path,
        WriteOp::WriteSection,
        HashMap::new(),
        "0.1.0".to_string(),
        "session-001".to_string(),
        "turn-001".to_string(),
    )
    .expect("构造 Manifest 应成功")
}

// ============================================================
// Round-trip 测试
// ============================================================

#[test]
fn test_manifest_roundtrip_json() {
    let tmp = TempDir::new().unwrap();
    let docx = write_fake_docx(&tmp, "thesis.docx", b"fake docx content");
    let manifest = make_manifest(docx.clone());

    // 序列化为 JSON 再反序列化，核心字段应保持一致
    let json = serde_json::to_string(&manifest).unwrap();
    let restored: Manifest = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.docx_path, manifest.docx_path);
    assert_eq!(restored.sha256_hex, manifest.sha256_hex);
    assert_eq!(restored.mtime, manifest.mtime);
    assert_eq!(restored.op, manifest.op);
    assert_eq!(restored.audit_version, manifest.audit_version);
    assert_eq!(restored.nonce, manifest.nonce);
    assert_eq!(restored.session_id, manifest.session_id);
    assert_eq!(restored.turn_id, manifest.turn_id);
}

#[test]
fn test_manifest_write_to_and_read_back() {
    let tmp = TempDir::new().unwrap();
    let docx = write_fake_docx(&tmp, "thesis.docx", b"docx bytes");
    let manifest = make_manifest(docx.clone());

    // 写到 .thesis/manifest.json
    let manifest_path = tmp.path().join(".thesis").join("manifest.json");
    manifest.write_to(&manifest_path).expect("write_to 应成功");

    // 目标文件应存在
    assert!(manifest_path.exists(), "manifest 文件应被创建");

    // 读回后 sha256 和 nonce 应与原始一致
    let content = std::fs::read_to_string(&manifest_path).unwrap();
    let restored: Manifest = serde_json::from_str(&content).unwrap();
    assert_eq!(restored.sha256_hex, manifest.sha256_hex);
    assert_eq!(restored.nonce, manifest.nonce);
}

#[test]
fn test_sha256_hex_is_64_chars() {
    let tmp = TempDir::new().unwrap();
    let docx = write_fake_docx(&tmp, "thesis.docx", b"hello thesis");
    let manifest = make_manifest(docx);

    // SHA-256 hex 固定 64 字符
    assert_eq!(manifest.sha256_hex.len(), 64);
    // 只含小写十六进制字符
    assert!(manifest.sha256_hex.chars().all(|c| c.is_ascii_hexdigit()));
}

// ============================================================
// verify_against_disk 测试
// ============================================================

#[test]
fn test_verify_ok_when_docx_unchanged() {
    let tmp = TempDir::new().unwrap();
    let docx = write_fake_docx(&tmp, "thesis.docx", b"unchanged content");
    let manifest = make_manifest(docx);

    // 文件未改动，验证应通过
    manifest
        .verify_against_disk()
        .expect("文件未改动，验证应通过");
}

#[test]
fn test_verify_detects_content_change() {
    let tmp = TempDir::new().unwrap();
    let docx = write_fake_docx(&tmp, "thesis.docx", b"original content");
    let manifest = make_manifest(docx.clone());

    // 修改 docx 内容（模拟 TOCTOU 攻击）
    // 等待 1ms 确保 mtime 一定不同（macOS HFS+ 精度 1s，但 APFS 纳秒级）
    std::fs::write(&docx, b"tampered content!!!").unwrap();

    let err = manifest
        .verify_against_disk()
        .expect_err("内容被修改，验证应失败");
    // 内容变了，应报 Sha256Mismatch
    assert!(
        matches!(err, thesis_manifest::TocTouViolation::Sha256Mismatch { .. }),
        "期望 Sha256Mismatch，实际: {err}"
    );
}

#[test]
fn test_verify_detects_mtime_change_without_content_change() {
    let tmp = TempDir::new().unwrap();
    let docx = write_fake_docx(&tmp, "thesis.docx", b"same content");
    let manifest = make_manifest(docx.clone());

    // 等待 100ms 让 mtime 前进——macOS APFS 纳秒精度，100ms 足够可靠地产生差异。
    // Linux ext4 默认纳秒精度同样可靠。
    // （如使用 HFS+ 或 FAT32 等低精度文件系统，此测试可能偶发性失败，
    //  届时可酌情跳过，但当前目标平台 APFS/ext4 均无此问题。）
    std::thread::sleep(std::time::Duration::from_millis(100));

    // 写入完全相同的内容：sha256 不变，但 mtime 已更新
    std::fs::write(&docx, b"same content").unwrap();

    // verify_against_disk 先检查 sha256（相同），再检查 mtime（不同）
    // 应报 MtimeMismatch，而非 Sha256Mismatch
    let err = manifest
        .verify_against_disk()
        .expect_err("mtime 已变化，验证应失败");
    assert!(
        matches!(err, thesis_manifest::TocTouViolation::MtimeMismatch { .. }),
        "期望 MtimeMismatch，实际: {err}"
    );
}

// ============================================================
// AuditLog 测试
// ============================================================

#[test]
fn test_audit_log_append_and_latest_for() {
    let tmp = TempDir::new().unwrap();
    let log_dir = tmp.path().join(".thesis");
    let audit_log = AuditLog::new(log_dir);

    let docx = write_fake_docx(&tmp, "thesis.docx", b"content v1");
    let manifest1 = make_manifest(docx.clone());

    audit_log.append(&manifest1).expect("第一次追加应成功");

    // 更新 docx 内容后构造第二个 manifest
    std::thread::sleep(std::time::Duration::from_millis(5));
    std::fs::write(&docx, b"content v2").unwrap();
    let manifest2 = make_manifest(docx.clone());
    audit_log.append(&manifest2).expect("第二次追加应成功");

    // latest_for 应返回最后一条（manifest2）
    let latest = audit_log
        .latest_for(&docx)
        .expect("读取应成功")
        .expect("应有结果");
    assert_eq!(latest.nonce, manifest2.nonce, "应返回最新的 manifest");
}

#[test]
fn test_audit_log_latest_for_returns_none_when_empty() {
    let tmp = TempDir::new().unwrap();
    let log_dir = tmp.path().join(".thesis");
    let audit_log = AuditLog::new(log_dir);

    let docx_path = tmp.path().join("nonexistent.docx");
    let result = audit_log.latest_for(&docx_path).expect("读取应成功");
    assert!(result.is_none(), "空日志应返回 None");
}

#[test]
fn test_audit_log_filters_by_path() {
    let tmp = TempDir::new().unwrap();
    let log_dir = tmp.path().join(".thesis");
    let audit_log = AuditLog::new(log_dir);

    let docx_a = write_fake_docx(&tmp, "a.docx", b"content a");
    let docx_b = write_fake_docx(&tmp, "b.docx", b"content b");

    let manifest_a = make_manifest(docx_a.clone());
    let manifest_b = make_manifest(docx_b.clone());

    audit_log.append(&manifest_a).unwrap();
    audit_log.append(&manifest_b).unwrap();

    // latest_for(a) 应只返回 manifest_a
    let latest_a = audit_log.latest_for(&docx_a).unwrap().unwrap();
    assert_eq!(latest_a.nonce, manifest_a.nonce);

    // latest_for(b) 应只返回 manifest_b
    let latest_b = audit_log.latest_for(&docx_b).unwrap().unwrap();
    assert_eq!(latest_b.nonce, manifest_b.nonce);
}

// ============================================================
// 并发 append 测试
// ============================================================

#[test]
fn test_concurrent_append_no_missing_entries() {
    // 验证多线程同时 append 不丢条目（POSIX O_APPEND 原子性）
    const THREAD_COUNT: usize = 8;
    const ENTRIES_PER_THREAD: usize = 10;

    let tmp = Arc::new(TempDir::new().unwrap());
    let log_dir = tmp.path().join(".thesis");
    std::fs::create_dir_all(&log_dir).unwrap();

    let log_dir_arc = Arc::new(log_dir);
    let tmp_arc = Arc::clone(&tmp);

    let mut handles = Vec::new();

    for thread_id in 0..THREAD_COUNT {
        let log_dir_clone = Arc::clone(&log_dir_arc);
        let tmp_clone = Arc::clone(&tmp_arc);

        let handle = std::thread::spawn(move || {
            let audit_log = AuditLog::new((*log_dir_clone).clone());

            for entry_id in 0..ENTRIES_PER_THREAD {
                // 每个线程写入独立的 docx 路径（用 thread_id + entry_id 区分）
                let docx_name = format!("thread_{thread_id}_entry_{entry_id}.docx");
                let docx = write_fake_docx(
                    &tmp_clone,
                    &docx_name,
                    format!("content {thread_id} {entry_id}").as_bytes(),
                );
                let manifest = make_manifest(docx);
                audit_log.append(&manifest).expect("并发 append 应成功");
            }
        });

        handles.push(handle);
    }

    // 等所有线程完成
    for handle in handles {
        handle.join().expect("线程不应 panic");
    }

    // 读取 audit-log，验证总行数等于 THREAD_COUNT * ENTRIES_PER_THREAD
    let log_path = (*log_dir_arc).join("audit-log.jsonl");
    let content = std::fs::read_to_string(&log_path).unwrap();
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();

    let expected = THREAD_COUNT * ENTRIES_PER_THREAD;
    assert_eq!(
        lines.len(),
        expected,
        "并发写入后应有 {expected} 条，实际 {}",
        lines.len()
    );

    // 每行都必须是合法 JSON
    for (i, line) in lines.iter().enumerate() {
        serde_json::from_str::<Manifest>(line)
            .unwrap_or_else(|e| panic!("第 {i} 行不是合法 Manifest JSON: {e}\n内容: {line}"));
    }
}
