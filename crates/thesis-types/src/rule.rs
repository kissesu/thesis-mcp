//! @file rule.rs
//! @description RuleId 枚举：覆盖 A/C/D/E/F 五系 13 条强制规则
//! @author Atlas.oi
//! @date 2026-05-17

use serde::{Deserialize, Serialize};

/// 规则 ID 枚举。
///
/// 序列化为 PLAN 中的稳定字符串 ID（如 "E.5.7"），可作为 `HashMap` 的 key
/// 并在 JSON 对象中以字符串形式出现，方便 manifest / audit-log 人工排查。
///
/// 命名规则：变体名去掉点号（Rust 标识符限制），通过 `serde(rename)` 还原。
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuleId {
    // ============================================
    // A 系：反 AI 痕迹（黑词 / em dash / CJK 间距）
    // ============================================
    #[serde(rename = "A.1")]
    A1,
    #[serde(rename = "A.5")]
    A5,
    #[serde(rename = "A.6")]
    A6,
    #[serde(rename = "A.7")]
    A7,
    #[serde(rename = "A.9")]
    A9,

    // ============================================
    // C 系：引用上标 [N] 与编号顺序
    // ============================================
    #[serde(rename = "C.1")]
    C1,
    #[serde(rename = "C.2")]
    C2,

    // ============================================
    // D 系：表格 cell pPr 清零
    // ============================================
    #[serde(rename = "D.9.1")]
    D91,
    #[serde(rename = "D.9.2")]
    D92,

    // ============================================
    // E 系：自动编号（章节号 / 参考文献）
    // ============================================
    #[serde(rename = "E.5.7")]
    E57,
    #[serde(rename = "E.5.8")]
    E58,

    // ============================================
    // F 系：修订模式合规
    // ============================================
    #[serde(rename = "F.5.1")]
    F51,
    #[serde(rename = "F.5.2")]
    F52,
}

impl RuleId {
    /// 返回规则的稳定字符串 ID（与 JSON 序列化保持一致）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::A1 => "A.1",
            Self::A5 => "A.5",
            Self::A6 => "A.6",
            Self::A7 => "A.7",
            Self::A9 => "A.9",
            Self::C1 => "C.1",
            Self::C2 => "C.2",
            Self::D91 => "D.9.1",
            Self::D92 => "D.9.2",
            Self::E57 => "E.5.7",
            Self::E58 => "E.5.8",
            Self::F51 => "F.5.1",
            Self::F52 => "F.5.2",
        }
    }
}

impl std::fmt::Display for RuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
