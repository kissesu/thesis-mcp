//! @file error.rs
//! @description 审计引擎统一错误类型
//! @author Atlas.oi
//! @date 2026-05-17

use std::path::PathBuf;

use thiserror::Error;

/// 审计引擎的错误枚举。
///
/// - `ParseError`：docx 解析阶段失败（XML 格式损坏、zip 损坏）
/// - `IoError`：文件读写失败
/// - `SchemaViolation`：OOXML 结构违反规范（缺少必要 part）
/// - `TocTouViolation`：文件在读取过程中被外部修改（sha256 不一致）
/// - `UnsupportedFormat`：不支持的文件格式（非 docx）
#[derive(Debug, Error)]
pub enum AuditError {
    /// docx 文件解析失败，包含底层原因
    #[error("解析 docx 失败：{path:?}：{reason}")]
    ParseError { path: PathBuf, reason: String },

    /// 文件 I/O 失败
    #[error("I/O 错误：{0}")]
    IoError(#[from] std::io::Error),

    /// OOXML 结构违反规范（缺少 main document part 等）
    #[error("OOXML 结构违规：{0}")]
    SchemaViolation(String),

    /// 文件在读取期间被外部修改（检测到哈希变化）
    #[error("文件在审计期间被修改（ToC-ToU 竞争）：{path:?}")]
    TocTouViolation { path: PathBuf },

    /// 不支持的文件格式（扩展名非 .docx 或 MIME 不匹配）
    #[error("不支持的文件格式：{0}")]
    UnsupportedFormat(String),

    /// ooxmlsdk SDK 内部错误（透传底层 SdkError Display）
    #[error("OOXML SDK 错误：{0}")]
    OoxmlSdk(String),
}

impl AuditError {
    /// 从 ooxmlsdk SdkError 构造，避免直接依赖 SdkError 类型以防止循环依赖
    #[must_use]
    pub fn from_sdk<E: std::fmt::Display>(e: E) -> Self {
        Self::OoxmlSdk(e.to_string())
    }
}
