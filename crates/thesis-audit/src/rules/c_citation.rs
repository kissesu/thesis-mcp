//! @file rules/c_citation.rs
//! @description C 系规则：引用标注格式检查（stub，待 L2.1b 实现）
//!
//! 规则覆盖：
//! - C.1：引用标注 [N] 必须以上标形式出现
//! - C.2：参考文献引用编号必须按出现顺序递增
//!
//! 完整实现需要：
//! - 遍历 run 的 RunProperties.verticalTextAlignment 判断上标
//! - 跨段落追踪已出现的 [N] 编号序列验证递增
//!
//! @author Atlas.oi
//! @date 2026-05-17

// stub: implement in L2.1b sub-task
// C.1 和 C.2 检测需要访问 RunProperties 的 verticalTextAlignment 字段，
// 以及跨段落状态追踪。在 document.rs 升级后实现。

use crate::document::DocParagraph;
use crate::rules::Violation;

/// C.1：引用标注必须以上标形式出现（stub）
// stub: implement in L2.1b sub-task
#[allow(dead_code)]
#[must_use]
pub fn check_c1_citation_superscript(_paragraphs: &[DocParagraph]) -> Vec<Violation> {
    todo!("L2.1b: C.1 — 扫描 run 的 RunProperties.verticalTextAlignment，检测 [N] 是否为上标")
}

/// C.2：参考文献引用编号顺序检查（stub）
// stub: implement in L2.1b sub-task
#[allow(dead_code)]
#[must_use]
pub fn check_c2_citation_order(_paragraphs: &[DocParagraph]) -> Vec<Violation> {
    todo!("L2.1b: C.2 — 跨段落追踪 [N] 引用顺序，验证首次出现递增")
}
