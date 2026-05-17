//! @file footnotes.rs
//! @description 脚注内容提取（stub，待 L2.1b 实现）
//!
//! 脚注存储于 FootnotesPart，正文通过 run 中的 WFootnoteReference 引用。
//!
//! @author Atlas.oi
//! @date 2026-05-17

// stub: implement in L2.1b sub-task

/// 脚注文本内容
#[allow(dead_code)]
pub struct FootnoteContent {
    pub footnote_id: i32,
    pub text: String,
}

/// 从 WordprocessingDocument 中提取所有脚注文本（stub）
// stub: implement in L2.1b sub-task
#[allow(dead_code)]
pub fn extract_footnotes(
    _package: &mut ooxmlsdk::parts::wordprocessing_document::WordprocessingDocument,
) -> Vec<FootnoteContent> {
    todo!("L2.1b: 访问 FootnotesPart，提取所有脚注段落文本")
}
