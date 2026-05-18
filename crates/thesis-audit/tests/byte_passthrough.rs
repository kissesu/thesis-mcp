//! @file tests/byte_passthrough.rs
//! @description HC-31 字节透传验证：audit_full 不修改 docx 文件内容
//!
//! 验证：在 audit_full 调用前后，docx 各 zip 条目的 sha256 保持不变。
//! 设计原则：审计是只读操作，任何 part 字节都不应被修改。
//!
//! @author Atlas.oi
//! @date 2026-05-18

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;

/// 计算字节切片的 sha256 hex。
fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

/// 提取 zip 中每个条目的 sha256，返回 `{entry_name → sha256_hex}` 映射。
fn zip_entry_hashes(zip_bytes: &[u8]) -> HashMap<String, String> {
    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor).expect("应能打开 zip");
    let mut map = HashMap::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).expect("应能读取 zip 条目");
        let name = entry.name().to_owned();
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).expect("应能读取条目内容");
        map.insert(name, sha256_hex(&buf));
    }

    map
}

/// 构建含 Header / Footer / 基本正文的 docx fixture。
fn build_docx_with_parts() -> Vec<u8> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <w:body>
    <w:p><w:r><w:t>正文段落一：本研究采用实验对比方法。</w:t></w:r></w:p>
    <w:p><w:r><w:t>正文段落二：实验结果表明该方法有效。</w:t></w:r></w:p>
    <w:sectPr>
      <w:headerReference w:type="default" r:id="rId2"/>
      <w:footerReference w:type="default" r:id="rId3"/>
    </w:sectPr>
  </w:body>
</w:document>"#;

    let header_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:p><w:r><w:t>页眉：论文题目</w:t></w:r></w:p>
</w:hdr>"#;

    let footer_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:p><w:r><w:t>第 1 页</w:t></w:r></w:p>
</w:ftr>"#;

    let numbering_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="%1."/>
      <w:lvlJc w:val="left"/>
    </w:lvl>
  </w:abstractNum>
  <w:num w:numId="1">
    <w:abstractNumId w:val="0"/>
  </w:num>
</w:numbering>"#;

    // 额外的未知 XML part（用于验证 ooxmlsdk 不会移除未知 part）
    let custom_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<root><data>这是自定义部分，审计后应保持不变</data></root>"#;

    let rels_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"
    Target="word/document.xml"/>
</Relationships>"#;

    let word_rels_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId2"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header"
    Target="header1.xml"/>
  <Relationship Id="rId3"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer"
    Target="footer1.xml"/>
  <Relationship Id="rId4"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering"
    Target="numbering.xml"/>
</Relationships>"#;

    let content_types_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml"
    ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
  <Override PartName="/word/header1.xml"
    ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>
  <Override PartName="/word/footer1.xml"
    ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>
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

    zip.start_file("word/header1.xml", opts).unwrap();
    zip.write_all(header_xml.as_bytes()).unwrap();

    zip.start_file("word/footer1.xml", opts).unwrap();
    zip.write_all(footer_xml.as_bytes()).unwrap();

    zip.start_file("word/numbering.xml", opts).unwrap();
    zip.write_all(numbering_xml.as_bytes()).unwrap();

    zip.start_file("word/custom_extra.xml", opts).unwrap();
    zip.write_all(custom_xml.as_bytes()).unwrap();

    zip.finish().unwrap().into_inner()
}

#[test]
fn test_byte_passthrough() {
    // ============================================
    // 1. 构建 fixture docx 并写入临时文件
    // ============================================
    let docx_bytes = build_docx_with_parts();
    let tmp = tempfile::NamedTempFile::new().expect("应能创建临时文件");
    std::fs::write(tmp.path(), &docx_bytes).expect("应能写入临时文件");

    // ============================================
    // 2. 记录 audit_full 调用前各 zip 条目的 sha256
    // ============================================
    let hashes_before = zip_entry_hashes(&docx_bytes);
    assert!(!hashes_before.is_empty(), "audit 前应能读取 zip 条目");

    // ============================================
    // 3. 执行全量审计（只读操作）
    // ============================================
    thesis_audit::audit_full(tmp.path()).expect("audit_full 应成功");

    // ============================================
    // 4. 重新读取文件，计算各条目 sha256，对比前后
    // 注意：audit_full 读取文件后不修改，临时文件内容不变
    // ============================================
    let after_bytes = std::fs::read(tmp.path()).expect("应能重读临时文件");
    let hashes_after = zip_entry_hashes(&after_bytes);

    // ============================================
    // 5. 所有条目的 sha256 必须完全一致
    // ============================================
    for (name, before_hash) in &hashes_before {
        let after_hash = hashes_after
            .get(name)
            .unwrap_or_else(|| panic!("条目 {name} 在 audit 后丢失"));
        assert_eq!(
            before_hash, after_hash,
            "audit 后条目 {name} 的字节发生了变化（HC-31 违规）"
        );
    }

    // 确保没有新增条目（audit 不应写入新文件）
    for name in hashes_after.keys() {
        assert!(
            hashes_before.contains_key(name),
            "audit 后出现了未知新条目：{name}（HC-31 违规）"
        );
    }
}
