//! @file severity.rs
//! @description Severity 枚举：违规严重度三级
//! @author Atlas.oi
//! @date 2026-05-17

use serde::{Deserialize, Serialize};

/// 违规严重度。
///
/// 写入工具策略：命中 `Critical` 即拒绝写回；`Warning` 放行并记录到 manifest；
/// `Info` 仅提示，不进 manifest 的 `rule_hits` 计数。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

impl Severity {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Critical => "Critical",
            Self::Warning => "Warning",
            Self::Info => "Info",
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
