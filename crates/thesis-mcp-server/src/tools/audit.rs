//! @file tools/audit.rs
//! @description `audit` 工具：审计指定 docx，返回 AuditResult JSON
//! @author Atlas.oi
//! @date 2026-05-17

use std::path::Path;

use anyhow::Result;
use schemars::JsonSchema;
use serde::Deserialize;
use thesis_types::AuditResult;

#[cfg(test)]
use chrono::Utc;
#[cfg(test)]
use thesis_types::{CheckRow, RuleId, Severity};

// ─── AuditEngine 抽象层 ──────────────────────────────────────────────────────

/// 审计引擎接口。
///
/// 两个实现：
/// - `RealAuditEngine`：调用 thesis-audit crate 的真实规则检查（生产使用）
/// - `StubAuditEngine`：总是返回 PASS（单元测试注入用）
pub trait AuditEngine: Send + Sync {
    fn audit_full(&self, docx_path: &Path) -> Result<AuditResult>;
}

/// 真实审计引擎：委托给 `thesis_audit::audit_full` 执行全量规则检查。
pub struct RealAuditEngine;

impl AuditEngine for RealAuditEngine {
    fn audit_full(&self, docx_path: &Path) -> Result<AuditResult> {
        thesis_audit::audit_full(docx_path).map_err(anyhow::Error::from)
    }
}

/// 桩实现：返回固定 PASS 结果，供单元测试注入（不依赖真实 docx 内容）。
///
/// 注：仅编译进测试目标（`#[cfg(test)]`），不进入发布二进制。
#[cfg(test)]
pub struct StubAuditEngine;

#[cfg(test)]
impl AuditEngine for StubAuditEngine {
    fn audit_full(&self, docx_path: &Path) -> Result<AuditResult> {
        tracing::debug!(
            "StubAuditEngine: 跳过真实审计，返回固定 PASS（path={:?}）",
            docx_path
        );
        Ok(AuditResult {
            docx_path: docx_path.to_path_buf(),
            sha256_hex: "0".repeat(64),
            audited_at: Utc::now(),
            audit_version: "stub-0.0.0".to_string(),
            passed: true,
            violations_count: 0,
            self_check_table: vec![make_stub_row()],
        })
    }
}

/// 构造一行示例自检记录（说明这是桩输出）。
#[cfg(test)]
fn make_stub_row() -> CheckRow {
    // 使用 A.1 作为占位规则 ID（任意有效变体均可）
    CheckRow {
        rule_id: RuleId::A1,
        severity: Severity::Info,
        item: "桩审计占位符".to_string(),
        expected: "thesis-audit 实现后替换".to_string(),
        actual: "StubAuditEngine 输出".to_string(),
        passed: true,
        locations: vec![],
    }
}

// ─── 工具接口 ────────────────────────────────────────────────────────────────

/// `audit` 工具输入参数。
#[derive(Debug, Deserialize, JsonSchema)]
pub struct AuditParams {
    /// 要审计的 docx 文件的绝对路径
    pub docx_path: String,
}

/// 执行 docx 审计并返回 `AuditResult`。
///
/// 业务逻辑：
/// 1. 验证 docx_path 是绝对路径且文件存在
/// 2. 调用 engine.audit_full(path) 获取审计结果
/// 3. 将 AuditResult 序列化为 JSON 字符串返回
pub fn run_audit(engine: &dyn AuditEngine, params: &AuditParams) -> Result<String> {
    let path = std::path::PathBuf::from(&params.docx_path);

    // 步骤 1：路径验证
    if !path.is_absolute() {
        anyhow::bail!("docx_path 必须是绝对路径: {}", params.docx_path);
    }
    if !path.exists() {
        anyhow::bail!("docx 文件不存在: {}", path.display());
    }

    // 步骤 2：执行审计
    tracing::info!("audit: 开始审计 {:?}", path);
    let result = engine.audit_full(&path)?;
    tracing::info!(
        "audit: 审计完成 passed={} violations={}",
        result.passed,
        result.violations_count
    );

    // 步骤 3：序列化为 JSON
    let json = serde_json::to_string_pretty(&result)?;
    Ok(json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_docx(dir: &Path) -> std::path::PathBuf {
        // 创建一个假 docx 文件（内容不重要，桩引擎不读取内容）
        let path = dir.join("test.docx");
        std::fs::write(&path, b"PK fake docx").expect("write fake docx");
        path
    }

    #[test]
    fn audit_stub_returns_pass() {
        let tmp = TempDir::new().expect("tempdir");
        let docx = make_docx(tmp.path());
        let engine = StubAuditEngine;

        let json = run_audit(
            &engine,
            &AuditParams {
                docx_path: docx.to_string_lossy().into_owned(),
            },
        )
        .expect("run_audit");

        // 确认返回的是合法 JSON
        let result: AuditResult = serde_json::from_str(&json).expect("parse AuditResult");
        assert!(result.passed);
        assert_eq!(result.violations_count, 0);
        assert_eq!(result.audit_version, "stub-0.0.0");
    }

    #[test]
    fn audit_fails_if_file_missing() {
        let engine = StubAuditEngine;
        let result = run_audit(
            &engine,
            &AuditParams {
                docx_path: "/nonexistent/file.docx".to_string(),
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn audit_fails_if_relative_path() {
        let engine = StubAuditEngine;
        let result = run_audit(
            &engine,
            &AuditParams {
                docx_path: "relative/file.docx".to_string(),
            },
        );
        assert!(result.is_err(), "相对路径应报错");
    }
}
