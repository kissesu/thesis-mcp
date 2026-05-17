//! @file textbox.rs
//! @description 文本框内容提取（stub，待 L2.1b 实现）
//!
//! 文本框是 `<w:pict>` 或 `mc:AlternateContent` 下的 `<wps:txbx>` 结构，
//! ooxmlsdk 类型化模型对此覆盖不完整，需要 quick-xml 流式处理。
//!
//! @author Atlas.oi
//! @date 2026-05-17

// stub: implement in L2.1b sub-task

/// 文本框中的段落文本（扁平提取）
#[allow(dead_code)]
pub struct TextboxContent {
    pub location: String,
    pub text: String,
}

/// 从 docx zip bytes 中流式扫描 mc:AlternateContent/wps:txbx 内的文本（stub）
// stub: implement in L2.1b sub-task
#[allow(dead_code)]
#[must_use]
pub fn extract_textbox_content(_docx_bytes: &[u8]) -> Vec<TextboxContent> {
    todo!("L2.1b: 用 quick-xml NsReader 流式扫描 mc:AlternateContent 内的文本框段落")
}
