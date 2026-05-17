//! @file headers_footers.rs
//! @description 页眉页脚内容提取（stub，待 L2.1b 实现）
//!
//! 页眉页脚在 OOXML 中存储于独立 Part（HeaderPart / FooterPart），
//! 通过 SectPr 中的 headerReference / footerReference 关联到主文档。
//!
//! @author Atlas.oi
//! @date 2026-05-17

// stub: implement in L2.1b sub-task

/// 页眉/页脚文本内容
#[allow(dead_code)]
pub struct HeaderFooterContent {
    pub kind: HeaderFooterKind,
    pub text: String,
}

#[allow(dead_code)]
pub enum HeaderFooterKind {
    Header,
    Footer,
}

/// 从 WordprocessingDocument 中提取所有页眉页脚文本（stub）
// stub: implement in L2.1b sub-task
#[allow(dead_code)]
pub fn extract_headers_footers(
    _package: &mut ooxmlsdk::parts::wordprocessing_document::WordprocessingDocument,
) -> Vec<HeaderFooterContent> {
    todo!("L2.1b: 通过 SectPr headerReference/footerReference 访问 HeaderPart/FooterPart")
}
