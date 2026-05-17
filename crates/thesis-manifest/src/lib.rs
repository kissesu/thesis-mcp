//! @file lib.rs
//! @description thesis-manifest 公开 API：Manifest 创建、原子写盘、TOCTOU 验证
//! @author Atlas.oi
//! @date 2026-05-17

pub mod store;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use thesis_types::{Manifest, RuleId, WriteOp};
use thiserror::Error;
use uuid::Uuid;

// ============================================================
// 错误类型
// ============================================================

/// TOCTOU 违规：manifest 写盘后、Stop hook 检查前，docx 被改动了。
#[derive(Debug, Error)]
pub enum TocTouViolation {
    /// 当前 mtime 比 manifest 里记录的更新
    #[error("docx mtime 不一致：manifest={manifest_mtime}, disk={disk_mtime}")]
    MtimeMismatch {
        manifest_mtime: DateTime<Utc>,
        disk_mtime: DateTime<Utc>,
    },
    /// 当前 sha256 与 manifest 里记录的不同
    #[error("docx sha256 不一致：manifest={manifest_sha256}, disk={disk_sha256}")]
    Sha256Mismatch {
        manifest_sha256: String,
        disk_sha256: String,
    },
    /// 读盘操作本身失败
    #[error("读取 docx 失败：{0}")]
    IoError(#[from] std::io::Error),
}

// ============================================================
// Manifest 扩展方法（构造 / 写盘 / 验证）
// ============================================================

/// 为 `thesis_types::Manifest` 提供构造与持久化能力的扩展 trait。
pub trait ManifestExt: Sized {
    /// 构造 Manifest：从磁盘读取 docx 内容，计算 sha256 + mtime。
    ///
    /// 业务流程：
    /// 1. 读取 docx 全部字节，计算 SHA-256 hex
    /// 2. 读取 docx 文件系统 mtime，转换为 UTC
    /// 3. 生成随机 nonce（v4 UUID）
    /// 4. 将上述数据与调用方传入的 op / session_id / turn_id 打包成 Manifest
    ///
    /// # Errors
    /// 读取 docx 或读取元数据失败时返回 `anyhow::Error`。
    fn new(
        docx_path: PathBuf,
        op: WriteOp,
        rule_hits: HashMap<RuleId, usize>,
        audit_version: String,
        session_id: String,
        turn_id: String,
    ) -> Result<Self, anyhow::Error>;

    /// 原子写入 manifest JSON 到 `path`。
    ///
    /// 采用先写临时文件、再 rename 的方式，保证部分写入不会留下损坏文件。
    /// 临时文件与目标文件在同一目录，确保 rename 是原子的（同文件系统）。
    ///
    /// # Errors
    /// 创建临时文件、序列化、rename 失败时返回 `anyhow::Error`。
    fn write_to(&self, path: &Path) -> Result<(), anyhow::Error>;

    /// 对比 manifest 与磁盘当前状态，检测 TOCTOU 违规。
    ///
    /// 检查顺序：
    /// 1. 先比 sha256（内容级别，最严格）
    /// 2. 再比 mtime（元数据级别，辅助诊断）
    ///
    /// # Errors
    /// 返回 `TocTouViolation` 表示具体违规类型。
    fn verify_against_disk(&self) -> Result<(), TocTouViolation>;
}

impl ManifestExt for Manifest {
    fn new(
        docx_path: PathBuf,
        op: WriteOp,
        rule_hits: HashMap<RuleId, usize>,
        audit_version: String,
        session_id: String,
        turn_id: String,
    ) -> Result<Self, anyhow::Error> {
        // 第一步：读取 docx 内容，计算 SHA-256
        let bytes = std::fs::read(&docx_path)?;
        let sha256_hex = compute_sha256_hex(&bytes);

        // 第二步：读取 mtime 并转换为 UTC DateTime
        let mtime = read_mtime(&docx_path)?;

        // 第三步：生成随机 nonce 绑定此 manifest 的唯一性
        let nonce = Uuid::new_v4();

        Ok(Manifest {
            docx_path,
            sha256_hex,
            mtime,
            op,
            rule_hits,
            audit_version,
            nonce,
            session_id,
            turn_id,
        })
    }

    fn write_to(&self, path: &Path) -> Result<(), anyhow::Error> {
        // 目标文件的父目录，临时文件必须在同一目录以保证 rename 原子性
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("manifest 路径没有父目录：{}", path.display()))?;
        std::fs::create_dir_all(parent)?;

        // 在同一目录创建临时文件，写入 JSON，然后 rename 到目标
        let mut tmp = NamedTempFile::new_in(parent)?;
        serde_json::to_writer(&mut tmp, self)?;
        tmp.persist(path)?;

        Ok(())
    }

    fn verify_against_disk(&self) -> Result<(), TocTouViolation> {
        // 第一步：从磁盘重新计算 sha256，与 manifest 对比（内容级别）
        let bytes = std::fs::read(&self.docx_path)?;
        let disk_sha256 = compute_sha256_hex(&bytes);
        if disk_sha256 != self.sha256_hex {
            return Err(TocTouViolation::Sha256Mismatch {
                manifest_sha256: self.sha256_hex.clone(),
                disk_sha256,
            });
        }

        // 第二步：比对 mtime（辅助检查，sha256 相同时理论上 mtime 也应一致）
        let disk_mtime = read_mtime(&self.docx_path).map_err(|e| {
            // read_mtime 内部只会因 IO 失败而报错，转换为 IoError 变体
            TocTouViolation::IoError(std::io::Error::other(e.to_string()))
        })?;
        if disk_mtime != self.mtime {
            return Err(TocTouViolation::MtimeMismatch {
                manifest_mtime: self.mtime,
                disk_mtime,
            });
        }

        Ok(())
    }
}

// ============================================================
// 内部辅助函数
// ============================================================

/// 计算字节切片的 SHA-256，返回小写 hex 字符串（64 字符）。
pub(crate) fn compute_sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// 读取文件的 mtime，转换为 UTC `DateTime<Utc>`。
pub(crate) fn read_mtime(path: &Path) -> Result<DateTime<Utc>, anyhow::Error> {
    let meta = std::fs::metadata(path)?;
    let system_time = meta.modified()?;
    // SystemTime → chrono UTC
    let duration = system_time
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| anyhow::anyhow!("mtime 早于 Unix 纪元: {e}"))?;
    // subsec_nanos() 返回 u32，直接转 u32 传给 from_timestamp；
    // as_secs() 返回 u64，通过 try_from 转 i64（理论上不会溢出，但保守处理）
    let nanos = duration.subsec_nanos(); // u32，直接用
    let secs = i64::try_from(duration.as_secs())?;
    DateTime::from_timestamp(secs, nanos)
        .ok_or_else(|| anyhow::anyhow!("无法将 mtime 转换为 DateTime<Utc>"))
}
