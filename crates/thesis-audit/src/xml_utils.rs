//! @file xml_utils.rs
//! @description 公共 XML 工具函数（zip 条目读取 + quick-xml 段落提取）
//!
//! 原 comments.rs / footnotes.rs 各自含一份相同实现，提取到此处消除重复。
//!
//! @author Atlas.oi
//! @date 2026-05-18

use quick_xml::Reader;
use quick_xml::events::Event;

/// 从 zip 字节中读取指定条目，返回原始字节。条目不存在时返回 None。
pub(crate) fn read_zip_entry(zip_bytes: &[u8], entry_name: &str) -> Option<Vec<u8>> {
    use std::io::Read;
    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor).ok()?;
    let mut entry = archive.by_name(entry_name).ok()?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// quick-xml 流式扫描：提取所有 `<w:p>` 内的 `<w:t>` 文本，
/// 返回 `(location_label, text)` 列表，location_label 格式为 `"{prefix}[{idx}]"`。
pub(crate) fn extract_paragraphs_from_xml(xml_bytes: &[u8], prefix: &str) -> Vec<(String, String)> {
    let mut reader = Reader::from_reader(xml_bytes);
    reader.config_mut().trim_text(false);

    let mut results = Vec::new();
    let mut buf = Vec::new();

    let mut para_idx: usize = 0;
    let mut in_p = false;
    let mut in_t = false;
    let mut current_text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                if local == b"p" {
                    in_p = true;
                    current_text.clear();
                } else if local == b"t" && in_p {
                    in_t = true;
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                if local == b"t" {
                    in_t = false;
                } else if local == b"p" && in_p {
                    results.push((
                        format!("{prefix}[{para_idx}]"),
                        std::mem::take(&mut current_text),
                    ));
                    para_idx += 1;
                    in_p = false;
                }
            }
            Ok(Event::Text(ref e)) if in_t && in_p => {
                if let Ok(text) = e.decode() {
                    current_text.push_str(&text);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    results
}

/// 去掉 XML 命名空间前缀，返回本地名称字节切片。
pub(crate) fn local_name(name: &[u8]) -> &[u8] {
    if let Some(pos) = name.iter().rposition(|&b| b == b':') {
        &name[pos + 1..]
    } else {
        name
    }
}
