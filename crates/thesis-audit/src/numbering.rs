//! @file numbering.rs
//! @description NumberingDefinitionsPart 解析：构建 numId → lvlText 映射
//!
//! 职责：
//! 1. 通过 ooxmlsdk `NumberingDefinitionsPart` 访问 `Numbering` 根元素
//! 2. 遍历 `abstractNum` → `Level` → `lvlText.val`，构建 abstractNumId → lvlTexts 映射
//! 3. 通过 `NumberingInstance`（numId → abstractNumId）构建最终 numId → lvlTexts 映射
//! 4. 暴露 `build_numbering_map` 供 e_format.rs 消费
//!
//! @author Atlas.oi
//! @date 2026-05-18

use std::collections::HashMap;
use std::io::Cursor;

use ooxmlsdk::parts::wordprocessing_document::WordprocessingDocument;

use crate::error::AuditError;

/// numId 对应的编号层级文本映射。
///
/// `lvl_texts[n]` 为第 n 层（0-indexed）的 `lvlText.val`（如 `"%1."` / `"[%1]"`）。
#[derive(Debug, Clone)]
pub struct NumIdLvlTexts {
    /// numId 值（来自 `<w:num w:numId="N">`）
    pub num_id: i32,
    /// 各层 lvlText，下标 = ilvl
    pub lvl_texts: Vec<String>,
}

/// 从 docx 字节构建 `numId → NumIdLvlTexts` 映射。
///
/// 业务流程：
/// 1. 从字节重新打开 WordprocessingDocument（只读，不修改）
/// 2. 取 main_document_part → numbering_definitions_part
/// 3. 解析 abstractNum 列表，构建 abstractNumId → lvl_texts 中间映射
/// 4. 遍历 num 实例，通过 abstractNumId 查中间映射，输出最终 numId → NumIdLvlTexts
///
/// # 错误
/// 文件不含 NumberingDefinitionsPart 时返回空 Vec（属于正常场景：无编号）。
pub fn build_numbering_map(docx_bytes: &[u8]) -> Result<Vec<NumIdLvlTexts>, AuditError> {
    let mut package =
        WordprocessingDocument::new(Cursor::new(docx_bytes)).map_err(AuditError::from_sdk)?;

    let main_part = package.main_document_part().map_err(AuditError::from_sdk)?;

    // numbering_definitions_part 是可选的；无编号文档直接返回空
    // ooxmlsdk optional child 方法返回 Option<T>（不是 Result）
    let Some(num_part) = main_part.numbering_definitions_part(&package) else {
        return Ok(Vec::new());
    };

    let numbering = num_part
        .root_element(&mut package)
        .map_err(AuditError::from_sdk)?;

    // ============================================
    // 第一步：构建 abstractNumId → lvl_texts 中间映射
    // 每个 abstractNum 有多个 Level（w:lvl）；Level.level_index 是 ilvl
    // ============================================
    let mut abstract_map: HashMap<i32, Vec<String>> = HashMap::new();

    for abstract_num in &numbering.w_abstract_num {
        let abstract_id = abstract_num.abstract_number_id;
        // 最多 9 层（ilvl 0-8），预分配
        let mut lvl_texts: Vec<String> = vec![String::new(); 9];

        for level in &abstract_num.w_lvl {
            #[allow(clippy::cast_sign_loss)] // level_index 语义上为非负
            let idx = level.level_index as usize;
            let text = level
                .level_text
                .as_ref()
                .and_then(|lt| lt.val.as_deref())
                .unwrap_or("")
                .to_owned();
            if idx < lvl_texts.len() {
                lvl_texts[idx] = text;
            }
        }

        // 去掉末尾的空字符串
        while lvl_texts.last().is_some_and(String::is_empty) {
            lvl_texts.pop();
        }

        abstract_map.insert(abstract_id, lvl_texts);
    }

    // ============================================
    // 第二步：遍历 NumberingInstance，通过 abstractNumId 查映射
    // ============================================
    let mut result = Vec::new();

    for num_inst in &numbering.w_num {
        let num_id = num_inst.number_id;
        let abstract_id = num_inst.abstract_num_id.val;

        let lvl_texts = abstract_map.get(&abstract_id).cloned().unwrap_or_default();

        result.push(NumIdLvlTexts { num_id, lvl_texts });
    }

    Ok(result)
}

/// 根据 numId 在映射列表中查找 lvlTexts。
///
/// 供 e_format.rs 调用，避免外部代码自行遍历。
#[must_use]
pub fn lookup_lvl_texts(map: &[NumIdLvlTexts], num_id: i32) -> Option<&NumIdLvlTexts> {
    map.iter().find(|n| n.num_id == num_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构建含 numbering.xml 的最小 docx fixture。
    fn build_docx_with_numbering(numbering_xml: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <w:body><w:p><w:r><w:t>内容</w:t></w:r></w:p><w:sectPr/></w:body>
</w:document>"#;

        let rels_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"
    Target="word/document.xml"/>
</Relationships>"#;

        let word_rels_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering"
    Target="numbering.xml"/>
</Relationships>"#;

        let content_types_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml"
    ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/numbering.xml"
    ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>
</Types>"#;

        let buf = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(buf);
        let opts = SimpleFileOptions::default();

        zip.start_file("_rels/.rels", opts).unwrap();
        zip.write_all(rels_xml.as_bytes()).unwrap();

        zip.start_file("[Content_Types].xml", opts).unwrap();
        zip.write_all(content_types_xml.as_bytes()).unwrap();

        zip.start_file("word/document.xml", opts).unwrap();
        zip.write_all(document_xml.as_bytes()).unwrap();

        zip.start_file("word/_rels/document.xml.rels", opts)
            .unwrap();
        zip.write_all(word_rels_xml.as_bytes()).unwrap();

        zip.start_file("word/numbering.xml", opts).unwrap();
        zip.write_all(numbering_xml.as_bytes()).unwrap();

        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn test_numbering_map_built() {
        // numbering.xml 定义 abstractNumId=0，numId=1，lvlText="%1."
        let numbering_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="%1."/>
      <w:lvlJc w:val="left"/>
    </w:lvl>
    <w:lvl w:ilvl="1">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="%1.%2"/>
      <w:lvlJc w:val="left"/>
    </w:lvl>
  </w:abstractNum>
  <w:num w:numId="1">
    <w:abstractNumId w:val="0"/>
  </w:num>
</w:numbering>"#;

        let docx_bytes = build_docx_with_numbering(numbering_xml);
        let map = build_numbering_map(&docx_bytes).expect("构建编号映射不应出错");

        assert!(!map.is_empty(), "映射不应为空");
        let entry = lookup_lvl_texts(&map, 1).expect("numId=1 应存在");
        assert_eq!(entry.num_id, 1);
        assert!(entry.lvl_texts.len() >= 2, "应有至少 2 层");
        assert_eq!(entry.lvl_texts[0], "%1.", "第 0 层 lvlText 应为 %1.");
        assert_eq!(entry.lvl_texts[1], "%1.%2", "第 1 层 lvlText 应为 %1.%2");
    }

    #[test]
    fn test_numbering_map_empty_when_no_numbering_part() {
        // 无 numbering.xml 的 docx → 返回空映射，不报错
        use crate::document::build_minimal_docx;
        let docx_bytes = build_minimal_docx("<w:p><w:r><w:t>内容</w:t></w:r></w:p>");
        let map = build_numbering_map(&docx_bytes).expect("无编号 docx 不应出错");
        assert!(map.is_empty(), "无编号 part 时映射应为空");
    }
}
