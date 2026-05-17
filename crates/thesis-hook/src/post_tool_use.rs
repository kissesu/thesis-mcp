//! @file post_tool_use.rs
//! @description PostToolUse hook：非 MCP 路径写入 docx 后的兜底审计
//!
//! 注：主要写入入口应走 MCP 工具（PreToolUse 拦截大部分非 MCP 写入）。
//! PostToolUse 是最后一道兜底：若 Claude 通过未被 PreToolUse 识别的路径写了 docx，
//! 在工具调用完成后触发外部审计并写 manifest。
//!
//! 覆盖约束：HC-14（PostToolUse on Write *.docx 永远不触发 — 兜底设计）
//!
//! 当前实现：
//! - 解析 stdin JSON，提取 tool_name / tool_input
//! - 若非 docx 写入 → exit 0（大多数情况）
//! - 若是 docx 写入 → 调用 thesis-audit::audit_full 并写 manifest
//!
//! @author Atlas.oi
//! @date 2026-05-17

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::Value;
use thesis_manifest::{ManifestExt as _, store::AuditLog};
use thesis_types::{Manifest, RuleId, WriteOp};

// ============================================================
// stdin JSON 结构
// ============================================================

#[derive(Debug, Deserialize)]
struct PostHookInput {
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    tool_name: String,
    #[serde(default)]
    tool_input: Value,
}

// ============================================================
// 主逻辑
// ============================================================

/// PostToolUse hook 入口，返回 exit code。
pub fn run() -> i32 {
    match run_inner() {
        Ok(code) => code,
        Err(e) => {
            // 兜底异常：不阻断（非 thesis 场景不阻断，HC-25 只适用于 thesis 域的 Stop hook）
            eprintln!("[thesis-hook/post] 内部错误，放行: {e}");
            0
        }
    }
}

fn run_inner() -> Result<i32, anyhow::Error> {
    let input = read_stdin_json()?;

    // 只关心 Write / Edit / MultiEdit / NotebookEdit 对 docx 的写入
    if !matches!(
        input.tool_name.as_str(),
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit"
    ) {
        return Ok(0);
    }

    let Some(file_path) = input
        .tool_input
        .get("file_path")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned)
    else {
        return Ok(0);
    };

    if !file_path.to_ascii_lowercase().ends_with(".docx") {
        return Ok(0);
    }

    // docx 写入事件：触发审计并写 manifest
    bootstrap_audit(&file_path, &input.session_id, &input.cwd)
}

/// 对刚写入的 docx 触发审计并写 manifest。
///
/// 这是 PreToolUse 拦截失败时的最后兜底路径。
fn bootstrap_audit(docx_path_str: &str, session_id: &str, cwd: &str) -> Result<i32, anyhow::Error> {
    let docx_path = if docx_path_str.starts_with('/') {
        Path::new(docx_path_str).to_path_buf()
    } else {
        Path::new(cwd).join(docx_path_str)
    };

    if !docx_path.exists() {
        eprintln!(
            "[thesis-hook/post] docx 不存在，跳过审计: {}",
            docx_path.display()
        );
        return Ok(0);
    }

    // 调用 thesis-audit 全量审计
    match thesis_audit::audit_full(&docx_path) {
        Ok(result) => {
            // 写入审计结果到 .thesis/audit-log.jsonl（通过 manifest）
            let thesis_dir: PathBuf = if cwd.is_empty() {
                PathBuf::from(".thesis")
            } else {
                PathBuf::from(cwd).join(".thesis")
            };

            // 从 AuditResult 的 self_check_table 构造 rule_hits
            let rule_hits: HashMap<RuleId, usize> = result
                .self_check_table
                .iter()
                .map(|row| (row.rule_id, row.locations.len()))
                .collect();

            let manifest = Manifest::new(
                docx_path,
                WriteOp::ExternalEdit, // 非 MCP 写入标记为 ExternalEdit（HC-30 TOCTOU 标记）
                rule_hits,
                result.audit_version,
                session_id.to_owned(),
                "post-tool-use".to_owned(), // turn_id 用固定值标记为 PostToolUse 产生
            )?;

            let audit_log = AuditLog::new(thesis_dir);
            audit_log.append(&manifest)?;

            if !result.passed {
                eprintln!(
                    "[thesis-hook/post] 审计失败：{} 处违规（docx: {}）",
                    result.violations_count,
                    result.docx_path.display()
                );
                // PostToolUse 只记录，不阻断（HC-14：PostToolUse 语义是兜底记录，
                // 真正的阻断由 PreToolUse 完成）
            }

            Ok(0)
        }
        Err(e) => {
            eprintln!("[thesis-hook/post] 审计失败（{docx_path_str}），放行: {e}");
            // PostToolUse 异常时放行（不是 Stop hook 的 fail-closed 场景）
            Ok(0)
        }
    }
}

/// 从 stdin 读取完整内容并解析。
fn read_stdin_json() -> Result<PostHookInput, anyhow::Error> {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    Ok(serde_json::from_str(&buf)?)
}
