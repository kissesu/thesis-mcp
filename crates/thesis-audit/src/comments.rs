//! @file comments.rs
//! @description 批注内容提取（stub，待 L2.1b 实现）
//!
//! 批注存储于独立 CommentsPart，正文通过 WCommentRangeStart/End 引用。
//! 投产时需检测是否存在遗留批注（应在提交前清除）。
//!
//! @author Atlas.oi
//! @date 2026-05-17

// stub: implement in L2.1b sub-task

use crate::rules::Violation;

/// 检测文档中是否存在遗留批注（stub）
// stub: implement in L2.1b sub-task
#[allow(dead_code)]
pub fn check_residual_comments(
    _package: &mut ooxmlsdk::parts::wordprocessing_document::WordprocessingDocument,
) -> Vec<Violation> {
    todo!("L2.1b: 访问 CommentsPart，检测是否存在未解决批注")
}
