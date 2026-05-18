//! @file styles.rs
//! @description 样式表解析（P3，轻实现）
//!
//! 从 StyleDefinitionsPart 提取样式定义，供下游规则查询字体/大小/颜色等属性。
//! 当前实现：加载样式列表，暴露按 styleId 查询的接口；
//! 字体/大小/颜色查询依赖 ooxmlsdk 深层字段，当前返回 None（文档化）。
//!
//! @author Atlas.oi
//! @date 2026-05-18

use std::io::Cursor;

use ooxmlsdk::parts::wordprocessing_document::WordprocessingDocument;

use crate::error::AuditError;

/// 样式定义条目（简化版，只包含当前规则检查所需字段）。
#[derive(Debug, Clone)]
pub struct StyleEntry {
    /// 样式 ID（如 "Heading1"、"Normal"）
    pub style_id: String,
    /// 样式名称（本地化名称，如 "标题 1"）
    pub style_name: String,
    /// 是否为段落样式
    pub is_paragraph: bool,
}

/// 从 docx 字节提取所有样式定义。
///
/// # 错误
/// 无 StyleDefinitionsPart 时返回空 Vec（正常场景）。
pub fn extract_styles(docx_bytes: &[u8]) -> Result<Vec<StyleEntry>, AuditError> {
    let mut package =
        WordprocessingDocument::new(Cursor::new(docx_bytes)).map_err(AuditError::from_sdk)?;

    let main_part = package.main_document_part().map_err(AuditError::from_sdk)?;

    // style_definitions_part 是可选 part，返回 Option<T>（不是 Result）
    let Some(style_part) = main_part.style_definitions_part(&package) else {
        return Ok(Vec::new());
    };

    let styles = style_part
        .root_element(&mut package)
        .map_err(AuditError::from_sdk)?;

    let mut result = Vec::new();

    for style in &styles.w_style {
        let style_id = style.style_id.clone().unwrap_or_default();

        // 样式名称：取 name.val
        let style_name = style
            .style_name
            .as_ref()
            .map(|n| n.val.clone())
            .unwrap_or_default();

        // 是否段落样式：type == "paragraph"
        let is_paragraph = style
            .r#type
            .as_ref()
            .is_some_and(|t| format!("{t:?}").to_ascii_lowercase().contains("paragraph"));

        result.push(StyleEntry {
            style_id,
            style_name,
            is_paragraph,
        });
    }

    Ok(result)
}

/// 按 styleId 查找样式条目。
#[must_use]
pub fn lookup_style<'a>(styles: &'a [StyleEntry], style_id: &str) -> Option<&'a StyleEntry> {
    styles.iter().find(|s| s.style_id == style_id)
}
