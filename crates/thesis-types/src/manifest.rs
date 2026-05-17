//! @file manifest.rs
//! @description Manifest 数据结构：写入工具产出的可信凭证，Stop hook 据此防 TOCTOU
//! @author Atlas.oi
//! @date 2026-05-17

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::rule::RuleId;

/// 写入操作类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WriteOp {
    /// `mcp__thesis__write_section`：写入新章节
    WriteSection,
    /// `mcp__thesis__revise`：对已有内容做修订（蓝色 ins/del）
    Revise,
    /// 外部工具产生的修改（兜底，理论上不应出现，出现即 TOCTOU）
    ExternalEdit,
}

/// docx 修改后的快照凭证。
///
/// 流程：
/// 1. 写入工具完成 OOXML 编辑 → 原子 rename 落盘
/// 2. 立即对新 docx 计算 sha256 + 读取 mtime
/// 3. 构造 Manifest 追加到 `.thesis/audit-log.jsonl`
/// 4. Stop hook 扫 `audit-log.jsonl` 拿最近一条，用 `verify_against_disk` 比对
///    现盘上的 sha256 / mtime；不一致 → TOCTOU 违规 → exit 2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// 目标 docx 绝对路径
    pub docx_path: PathBuf,
    /// docx 文件的 SHA-256，小写 hex（恰好 64 字符）
    pub sha256_hex: String,
    /// docx 文件的 mtime（UTC，写盘那一刻读取）
    pub mtime: DateTime<Utc>,
    /// 本次写入的操作类型
    pub op: WriteOp,
    /// 本次审计中各规则命中次数（rule_id -> count）
    pub rule_hits: HashMap<RuleId, usize>,
    /// 审计引擎语义版本号，用于排查规则变更带来的回归
    pub audit_version: String,
    /// 本轮写入随机生成的 nonce，绑定 manifest 唯一性
    pub nonce: Uuid,
    /// Claude 会话 ID（从 hook stdin / 环境变量取）
    pub session_id: String,
    /// Claude 当前 turn ID（每个 user prompt 一个）
    pub turn_id: String,
}
