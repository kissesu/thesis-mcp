//! @file audit.rs
//! @description 审计结果数据结构：自检表单行 + 整体 AuditResult
//! @author Atlas.oi
//! @date 2026-05-17

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::rule::RuleId;
use crate::severity::Severity;

/// 自检表单行。
///
/// 每行对应一条规则的一次检查记录，由 thesis-audit 产出，最终：
/// - 进入 `AuditResult::self_check_table`
/// - 由 thesis-hook 转换为 markdown 表注入 PostToolUse additionalContext
/// - 写入 `.thesis/audit-log.jsonl` 供人工排查
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckRow {
    pub rule_id: RuleId,
    pub severity: Severity,
    /// 检查项的人读名称（例："章节号自动编号"）
    pub item: String,
    /// 期望状态（例："numbering.xml 含 lvlText=%1."）
    pub expected: String,
    /// 实际状态（例："段落无 numPr"）
    pub actual: String,
    /// 是否通过此项检查
    pub passed: bool,
    /// 命中位置列表（例：["body/p[3]", "tbl[2]/tr[0]/tc[1]/p[0]"]）
    pub locations: Vec<String>,
}

/// 一次完整审计的结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditResult {
    pub docx_path: PathBuf,
    /// 被审计 docx 的 sha256 hex，与 Manifest 中保持一致
    pub sha256_hex: String,
    /// 审计执行时间（UTC）
    pub audited_at: DateTime<Utc>,
    /// 审计引擎版本号
    pub audit_version: String,
    /// 总判定：所有 Critical 项 passed=true 即 true
    pub passed: bool,
    /// Critical + Warning 命中行总数（不含 Info）
    pub violations_count: usize,
    /// 完整自检表
    pub self_check_table: Vec<CheckRow>,
}
