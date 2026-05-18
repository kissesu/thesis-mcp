//! @file tracked_changes.rs
//! @description F.5.1 / F.5.2：修订痕迹检测
//!
//! 规则覆盖：
//! - F.5.2：`<w:ins>` 内的 run 含 `<w:strike/>` → 遗留删除线（残留修订痕迹）
//! - F.5.1：`<w:ins>` 内的 run 含非蓝色 color 属性（Word 默认修订插入色为蓝色 0070C0）
//!
//! 实现策略：quick-xml 流式扫描 document.xml，状态机追踪进入 `<w:ins>` 后的 rPr。
//! 蓝色判定：val == "0070C0"（大小写不敏感）或 "auto" 不触发 F.5.1。
//!
//! @author Atlas.oi
//! @date 2026-05-18

use quick_xml::Reader;
use quick_xml::events::Event;
use thesis_types::{RuleId, Severity};

use crate::rules::Violation;

/// 扫描 docx 字节中 document.xml 的修订痕迹（F.5.1 / F.5.2）。
#[must_use]
pub fn check_tracked_changes(docx_bytes: &[u8]) -> Vec<Violation> {
    let Some(xml_bytes) = read_zip_entry(docx_bytes, "word/document.xml") else {
        return Vec::new();
    };

    scan_tracked_changes(&xml_bytes)
}

fn read_zip_entry(zip_bytes: &[u8], entry_name: &str) -> Option<Vec<u8>> {
    use std::io::Read;
    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor).ok()?;
    let mut entry = archive.by_name(entry_name).ok()?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// 事件分类：只关心 Start / Empty / End。
#[derive(Debug)]
enum ParsedEvent {
    Start {
        local: String,
        // L4.2: Start 属性当前不被状态机消费；保留字段供后续扩展（如 ins w:id 追踪）
        #[allow(dead_code)]
        attrs: Vec<(String, String)>,
    },
    Empty {
        local: String,
        attrs: Vec<(String, String)>,
    },
    End {
        local: String,
    },
    Other,
}

/// 将字节解析为 `ParsedEvent`，不持有 `&buf` 借用（提前克隆所有数据）。
fn to_parsed(event: &Event<'_>) -> ParsedEvent {
    match event {
        Event::Start(e) => {
            let local = String::from_utf8_lossy(local_name(e.name().as_ref())).into_owned();
            let attrs = extract_attrs(e);
            ParsedEvent::Start { local, attrs }
        }
        Event::Empty(e) => {
            let local = String::from_utf8_lossy(local_name(e.name().as_ref())).into_owned();
            let attrs = extract_attrs(e);
            ParsedEvent::Empty { local, attrs }
        }
        Event::End(e) => {
            let local = String::from_utf8_lossy(local_name(e.name().as_ref())).into_owned();
            ParsedEvent::End { local }
        }
        _ => ParsedEvent::Other,
    }
}

fn extract_attrs(e: &quick_xml::events::BytesStart<'_>) -> Vec<(String, String)> {
    e.attributes()
        .filter_map(Result::ok)
        .map(|a| {
            let key = String::from_utf8_lossy(local_name(a.key.as_ref())).into_owned();
            let val = String::from_utf8_lossy(&a.value).into_owned();
            (key, val)
        })
        .collect()
}

/// quick-xml 状态机：扫描修订痕迹，不跨事件持有借用。
fn scan_tracked_changes(xml_bytes: &[u8]) -> Vec<Violation> {
    let mut reader = Reader::from_reader(xml_bytes);
    reader.config_mut().trim_text(true);

    let mut violations = Vec::new();
    let mut buf = Vec::new();

    let mut ins_depth: i32 = 0;
    let mut in_rpr = false;
    let mut ins_index: usize = 0;

    loop {
        // 每次读取一个事件，立即克隆所有需要的数据，释放 buf 借用
        let parsed = {
            let event = reader.read_event_into(&mut buf);
            match &event {
                Ok(Event::Eof) | Err(_) => break,
                Ok(e) => to_parsed(e),
            }
        };
        buf.clear();

        match parsed {
            ParsedEvent::Start { ref local, .. } => {
                match local.as_str() {
                    "ins" => {
                        ins_depth += 1;
                    }
                    "rPr" if ins_depth > 0 => {
                        in_rpr = true;
                    }
                    // del / moveFrom / moveTo 不检查 F.5.1，不计入 ins_depth
                    _ => {}
                }
            }
            ParsedEvent::Empty {
                ref local,
                ref attrs,
            } => match local.as_str() {
                "strike" if in_rpr && ins_depth > 0 => {
                    violations.push(Violation {
                        rule_id: RuleId::F52,
                        severity: Severity::Critical,
                        location: format!("body/ins[{ins_index}]/rPr/strike"),
                        actual: "插入修订内含 w:strike 删除线（遗留修订痕迹）".to_owned(),
                    });
                }
                "color" if in_rpr && ins_depth > 0 => {
                    let val = attrs
                        .iter()
                        .find(|(k, _)| k == "val")
                        .map(|(_, v)| v.clone())
                        .unwrap_or_default();

                    if !is_blue_or_auto(&val) {
                        violations.push(Violation {
                            rule_id: RuleId::F51,
                            severity: Severity::Warning,
                            location: format!("body/ins[{ins_index}]/rPr/color"),
                            actual: format!("插入修订颜色非蓝色：val=\"{val}\""),
                        });
                    }
                }
                _ => {}
            },
            ParsedEvent::End { ref local } => match local.as_str() {
                "ins" if ins_depth > 0 => {
                    ins_depth -= 1;
                    if ins_depth == 0 {
                        ins_index += 1;
                    }
                }
                "rPr" => {
                    in_rpr = false;
                }
                _ => {}
            },
            ParsedEvent::Other => {}
        }
    }

    violations
}

/// 判断颜色值是否为蓝色或 auto。
fn is_blue_or_auto(val: &str) -> bool {
    let upper = val.to_ascii_uppercase();
    upper == "AUTO" || upper == "0070C0" || upper == "4472C4"
}

fn local_name(name: &[u8]) -> &[u8] {
    if let Some(pos) = name.iter().rposition(|&b| b == b':') {
        &name[pos + 1..]
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_docx_with_xml(document_xml: &str) -> Vec<u8> {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let rels_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"
    Target="word/document.xml"/>
</Relationships>"#;

        let word_rels_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
</Relationships>"#;

        let content_types_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml"
    ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
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

        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn test_tracked_changes_strike_residual() {
        let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:ins w:id="1" w:author="Test" w:date="2026-01-01T00:00:00Z">
        <w:r>
          <w:rPr>
            <w:strike/>
          </w:rPr>
          <w:t>删除线文本</w:t>
        </w:r>
      </w:ins>
    </w:p>
    <w:sectPr/>
  </w:body>
</w:document>"#;

        let docx_bytes = build_docx_with_xml(document_xml);
        let violations = check_tracked_changes(&docx_bytes);

        let f52: Vec<_> = violations
            .iter()
            .filter(|v| v.rule_id == RuleId::F52)
            .collect();
        assert!(
            !f52.is_empty(),
            "strike 在 ins 内应触发 F.5.2，实际：{violations:?}"
        );
    }

    #[test]
    fn test_tracked_changes_non_blue_ins() {
        let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:ins w:id="1" w:author="Test" w:date="2026-01-01T00:00:00Z">
        <w:r>
          <w:rPr>
            <w:color w:val="FF0000"/>
          </w:rPr>
          <w:t>红色修订文本</w:t>
        </w:r>
      </w:ins>
    </w:p>
    <w:sectPr/>
  </w:body>
</w:document>"#;

        let docx_bytes = build_docx_with_xml(document_xml);
        let violations = check_tracked_changes(&docx_bytes);

        let f51: Vec<_> = violations
            .iter()
            .filter(|v| v.rule_id == RuleId::F51)
            .collect();
        assert!(
            !f51.is_empty(),
            "非蓝色 ins 应触发 F.5.1，实际：{violations:?}"
        );
        assert!(f51[0].actual.contains("FF0000"));
    }

    #[test]
    fn test_tracked_changes_blue_ins_no_violation() {
        let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:ins w:id="1" w:author="Test" w:date="2026-01-01T00:00:00Z">
        <w:r>
          <w:rPr>
            <w:color w:val="0070C0"/>
          </w:rPr>
          <w:t>蓝色修订文本（合规）</w:t>
        </w:r>
      </w:ins>
    </w:p>
    <w:sectPr/>
  </w:body>
</w:document>"#;

        let docx_bytes = build_docx_with_xml(document_xml);
        let violations = check_tracked_changes(&docx_bytes);
        let f51: Vec<_> = violations
            .iter()
            .filter(|v| v.rule_id == RuleId::F51)
            .collect();
        assert!(
            f51.is_empty(),
            "蓝色 ins 不应触发 F.5.1，实际：{violations:?}"
        );
    }

    #[test]
    fn test_clean_doc_no_tracked_changes() {
        let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>正常内容</w:t></w:r></w:p>
    <w:sectPr/>
  </w:body>
</w:document>"#;

        let docx_bytes = build_docx_with_xml(document_xml);
        let violations = check_tracked_changes(&docx_bytes);
        assert!(
            violations.is_empty(),
            "无修订文档不应有 F 系违规：{violations:?}"
        );
    }
}
