//! @file styles.rs
//! @description 样式表解析（stub，待 L2.1b 实现）
//!
//! 职责：从 StyleDefinitionsPart 中提取所有样式定义，
//! 供规则层验证段落样式是否符合模板要求。
//!
//! @author Atlas.oi
//! @date 2026-05-17

// stub: implement in L2.1b sub-task

/// 样式定义条目
#[allow(dead_code)]
pub struct StyleEntry {
    pub style_id: String,
    pub style_name: String,
    /// 是否为段落样式
    pub is_paragraph: bool,
}

/// 从 WordprocessingDocument 中提取所有样式定义（stub）
// stub: implement in L2.1b sub-task
#[allow(dead_code)]
pub fn extract_styles(
    _package: &mut ooxmlsdk::parts::wordprocessing_document::WordprocessingDocument,
) -> Vec<StyleEntry> {
    todo!("L2.1b: 访问 StyleDefinitionsPart，提取 StyleEntry 列表")
}
