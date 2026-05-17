//! @file tracked_changes.rs
//! @description 修订模式检测（stub，待 L2.1b 实现）
//!
//! 职责：检测 docx 中是否存在未接受的修订痕迹（Track Changes），
//! 对应 F.5.1 / F.5.2 规则。
//!
//! 检测方式：扫描 body_choice 中的 WIns / WDel / WMoveFrom / WMoveTo 变体。
//!
//! @author Atlas.oi
//! @date 2026-05-17

// stub: implement in L2.1b sub-task

use crate::rules::Violation;

/// 检测 docx 中是否存在未接受的修订痕迹（stub）
// stub: implement in L2.1b sub-task
#[allow(dead_code)]
#[must_use]
pub fn check_tracked_changes(
    _body_choice: &[ooxmlsdk::schemas::schemas_openxmlformats_org_wordprocessingml_2006_main::BodyChoice],
) -> Vec<Violation> {
    todo!("L2.1b: F.5.1/F.5.2 — 扫描 BodyChoice::WIns/WDel/WMoveFrom/WMoveTo 变体")
}
