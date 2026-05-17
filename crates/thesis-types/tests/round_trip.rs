//! @file round_trip.rs
//! @description thesis-types 各类型的 serde JSON round-trip 验证
//! @author Atlas.oi
//! @date 2026-05-17

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use thesis_types::{AuditResult, CheckRow, Manifest, RuleId, Severity, WriteOp};
use uuid::Uuid;

#[test]
fn rule_id_serializes_to_dotted_string() {
    // 验证 enum variant 序列化后是稳定的 "X.Y[.Z]" 形式
    let serialized = serde_json::to_string(&RuleId::E57).unwrap();
    assert_eq!(serialized, r#""E.5.7""#);
}

#[test]
fn rule_id_round_trip_for_all_variants() {
    // 13 条规则全部走一遍 round-trip，防止漏改
    let all = [
        (RuleId::A1, "A.1"),
        (RuleId::A5, "A.5"),
        (RuleId::A6, "A.6"),
        (RuleId::A7, "A.7"),
        (RuleId::A9, "A.9"),
        (RuleId::C1, "C.1"),
        (RuleId::C2, "C.2"),
        (RuleId::D91, "D.9.1"),
        (RuleId::D92, "D.9.2"),
        (RuleId::E57, "E.5.7"),
        (RuleId::E58, "E.5.8"),
        (RuleId::F51, "F.5.1"),
        (RuleId::F52, "F.5.2"),
    ];
    for (rid, expected) in all {
        assert_eq!(rid.as_str(), expected);
        let j = serde_json::to_string(&rid).unwrap();
        assert_eq!(j, format!("\"{expected}\""));
        let back: RuleId = serde_json::from_str(&j).unwrap();
        assert_eq!(back, rid);
    }
}

#[test]
fn rule_id_works_as_hashmap_key_in_json() {
    // Manifest::rule_hits 字段依赖 RuleId 可作为 JSON object key
    let mut hits: HashMap<RuleId, usize> = HashMap::new();
    hits.insert(RuleId::E57, 2);
    hits.insert(RuleId::F52, 1);
    let json = serde_json::to_string(&hits).unwrap();
    // JSON 对象 key 是字符串，必须能由 enum 转出
    assert!(json.contains("\"E.5.7\":2") || json.contains("\"E.5.7\": 2"));
    let parsed: HashMap<RuleId, usize> = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, hits);
}

#[test]
fn severity_round_trip() {
    for s in [Severity::Critical, Severity::Warning, Severity::Info] {
        let j = serde_json::to_string(&s).unwrap();
        let back: Severity = serde_json::from_str(&j).unwrap();
        assert_eq!(back, s);
        assert_eq!(s.to_string(), s.as_str());
    }
}

#[test]
fn write_op_round_trip() {
    for op in [
        WriteOp::WriteSection,
        WriteOp::Revise,
        WriteOp::ExternalEdit,
    ] {
        let j = serde_json::to_string(&op).unwrap();
        let back: WriteOp = serde_json::from_str(&j).unwrap();
        assert_eq!(back, op);
    }
}

#[test]
fn manifest_round_trip_preserves_all_fields() {
    let mut hits = HashMap::new();
    hits.insert(RuleId::E57, 3);
    hits.insert(RuleId::D91, 1);

    let original = Manifest {
        docx_path: PathBuf::from("/tmp/test.docx"),
        sha256_hex: "0".repeat(64),
        mtime: Utc.with_ymd_and_hms(2026, 5, 17, 10, 0, 0).unwrap(),
        op: WriteOp::WriteSection,
        rule_hits: hits.clone(),
        audit_version: "0.1.0".into(),
        nonce: Uuid::nil(),
        session_id: "sess-1".into(),
        turn_id: "turn-1".into(),
    };

    let json = serde_json::to_string(&original).unwrap();
    let parsed: Manifest = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.docx_path, original.docx_path);
    assert_eq!(parsed.sha256_hex, original.sha256_hex);
    assert_eq!(parsed.mtime, original.mtime);
    assert_eq!(parsed.op, original.op);
    assert_eq!(parsed.rule_hits, original.rule_hits);
    assert_eq!(parsed.audit_version, original.audit_version);
    assert_eq!(parsed.nonce, original.nonce);
    assert_eq!(parsed.session_id, original.session_id);
    assert_eq!(parsed.turn_id, original.turn_id);
}

#[test]
fn audit_result_round_trip_with_check_rows() {
    let original = AuditResult {
        docx_path: PathBuf::from("/tmp/t.docx"),
        sha256_hex: "a".repeat(64),
        audited_at: Utc.with_ymd_and_hms(2026, 5, 17, 9, 30, 0).unwrap(),
        audit_version: "0.1.0".into(),
        passed: false,
        violations_count: 1,
        self_check_table: vec![CheckRow {
            rule_id: RuleId::E57,
            severity: Severity::Critical,
            item: "章节号自动编号".into(),
            expected: "lvlText=%1.".into(),
            actual: "段落无 numPr".into(),
            passed: false,
            locations: vec!["body/p[3]".into()],
        }],
    };
    let json = serde_json::to_string(&original).unwrap();
    let parsed: AuditResult = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.passed, original.passed);
    assert_eq!(parsed.violations_count, original.violations_count);
    assert_eq!(parsed.self_check_table.len(), 1);
    assert_eq!(parsed.self_check_table[0].rule_id, RuleId::E57);
    assert_eq!(parsed.self_check_table[0].severity, Severity::Critical);
    assert!(!parsed.self_check_table[0].passed);
}

#[test]
fn rule_id_display_matches_as_str() {
    assert_eq!(format!("{}", RuleId::D91), "D.9.1");
    assert_eq!(format!("{}", RuleId::F52), "F.5.2");
}

#[test]
fn sha256_hex_is_64_chars() {
    // 不强约束 newtype，但通过测试明确合同：sha256_hex 字段必须为 64 字符 hex
    let m = Manifest {
        docx_path: PathBuf::from("/x.docx"),
        sha256_hex: "0".repeat(64),
        mtime: Utc.with_ymd_and_hms(2026, 5, 17, 0, 0, 0).unwrap(),
        op: WriteOp::Revise,
        rule_hits: HashMap::new(),
        audit_version: "0.1.0".into(),
        nonce: Uuid::nil(),
        session_id: String::new(),
        turn_id: String::new(),
    };
    assert_eq!(m.sha256_hex.len(), 64);
}
