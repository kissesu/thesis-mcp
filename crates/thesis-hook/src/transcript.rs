//! @file transcript.rs
//! @description 解析 CC transcript.jsonl，判断当前会话是否处于 thesis 域（SC-2）
//!
//! thesis 域识别标准（三选一满足即成立）：
//! 1. 任意工具调用名以 `mcp__thesis__` 开头
//! 2. 任意用户消息含 `/thesis` 或 `论文`（大小写不敏感）
//! 3. 任意 system reminder 消息含 `thesis`（大小写不敏感）
//!
//! 设计约束：
//! - 不依赖 `<command-name>/thesis</command-name>` 字符串（SC-2 要求）
//! - 别名调用 / 复制 skill 内容执行也能识别
//! - transcript.jsonl 每行是一个 JSON 事件对象
//!
//! @author Atlas.oi
//! @date 2026-05-17

use std::io::{BufRead, BufReader};
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

// ============================================================
// transcript 事件结构（只反序列化需要的字段）
// ============================================================

/// CC transcript 中单条事件的最小结构。
///
/// transcript.jsonl 的完整 schema 对我们不透明，
/// 只提取 type / role / content / timestamp 四个可能字段。
#[derive(Debug, Deserialize)]
pub struct TranscriptEvent {
    /// 事件类型：`"message"` / `"tool_use"` / `"tool_result"` / 等
    #[serde(default)]
    pub r#type: String,

    /// 消息角色（仅 message 类型）：`"user"` / `"assistant"` / `"system"`
    /// 目前只用于 Debug 输出，不作业务判断
    #[serde(default)]
    #[allow(dead_code)]
    pub role: String,

    /// 消息内容，可能是字符串或对象数组
    #[serde(default)]
    pub content: Value,

    /// 事件时间戳（RFC 3339 字符串 → `DateTime<Utc>`）
    pub timestamp: Option<DateTime<Utc>>,
}

// ============================================================
// 解析结果
// ============================================================

/// transcript 解析摘要。
pub struct TranscriptSummary {
    /// 当前会话是否处于 thesis 域
    pub is_thesis_domain: bool,
    /// 第一条事件的时间戳（用于 mtime 扫描的基准时间）
    pub session_start: Option<DateTime<Utc>>,
}

// ============================================================
// 主解析函数
// ============================================================

/// 读取并解析 `transcript_path`，返回 `TranscriptSummary`。
///
/// 业务流程：
/// 1. 打开文件（不存在 → session_start=None, thesis=false）
/// 2. 逐行解析 JSON 事件
/// 3. 记录第一行的 timestamp 作为 session_start
/// 4. 检查每行是否命中 thesis 域信号（三条规则）
///
/// # Errors
/// 仅在文件 IO 失败时返回 Err；JSON 解析失败的单行直接跳过（鲁棒）。
pub fn parse(transcript_path: &Path) -> Result<TranscriptSummary, anyhow::Error> {
    if !transcript_path.exists() {
        return Ok(TranscriptSummary {
            is_thesis_domain: false,
            session_start: None,
        });
    }

    let file = std::fs::File::open(transcript_path)?;
    let reader = BufReader::new(file);

    let mut is_thesis_domain = false;
    let mut session_start: Option<DateTime<Utc>> = None;
    let mut first_line = true;

    for line_result in reader.lines() {
        let Ok(line) = line_result else {
            continue; // IO 错误跳行，继续处理
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // 第一行：无论能否解析都尝试拿 timestamp 作为 session_start
        if first_line {
            first_line = false;
            if let Ok(event) = serde_json::from_str::<TranscriptEvent>(trimmed) {
                session_start = event.timestamp;
            }
        }

        // 尝试解析为 TranscriptEvent，失败跳过（transcript 格式可能变化）
        let event: TranscriptEvent = if let Ok(e) = serde_json::from_str(trimmed) {
            e
        } else {
            // 无法解析为结构体，但仍对原始行做关键词扫描（兜底 SC-2）
            if is_thesis_signal_raw(trimmed) {
                is_thesis_domain = true;
            }
            continue;
        };

        if !is_thesis_domain && is_thesis_signal_event(&event) {
            is_thesis_domain = true;
        }
    }

    Ok(TranscriptSummary {
        is_thesis_domain,
        session_start,
    })
}

// ============================================================
// thesis 域检测逻辑
// ============================================================

/// 对已解析的 TranscriptEvent 检查 thesis 域信号。
fn is_thesis_signal_event(event: &TranscriptEvent) -> bool {
    // 规则 1：工具调用名以 mcp__thesis__ 开头
    if event.r#type == "tool_use" {
        if let Some(name) = event.content.get("name").and_then(|v| v.as_str())
            && name.starts_with("mcp__thesis__")
        {
            return true;
        }
        // 某些格式把 tool_name 直接放在顶层
        if let Some(name) = event.content.as_str()
            && name.starts_with("mcp__thesis__")
        {
            return true;
        }
    }

    // 规则 2/3：user 消息或 system 消息含 thesis 关键词
    let content_str = event_content_text(event);
    is_thesis_keyword(&content_str)
}

/// 对无法解析为结构体的原始 JSON 字符串做关键词扫描（兜底 SC-2）。
fn is_thesis_signal_raw(raw: &str) -> bool {
    // 工具调用模式：JSON 中含 "mcp__thesis__" 字符串
    if raw.contains("mcp__thesis__") {
        return true;
    }
    is_thesis_keyword(raw)
}

/// 检查字符串是否含 thesis 域关键词（大小写不敏感）。
///
/// 关键词：/thesis, 论文
/// 不包含 "docx" / "word" — 这些在 pre_tool_use 里是拦截信号，
/// 但在 transcript 里太宽泛，会把普通提到 docx 格式的对话误标为 thesis 域。
fn is_thesis_keyword(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("/thesis") || lower.contains("论文")
}

/// 从 TranscriptEvent 提取可读文本（content 可能是 String 或 Array）。
fn event_content_text(event: &TranscriptEvent) -> String {
    match &event.content {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            // content 是块数组（type/text 对象）
            arr.iter()
                .filter_map(|block| {
                    block
                        .get("text")
                        .and_then(|t| t.as_str())
                        .map(ToOwned::to_owned)
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
        Value::Object(_) => {
            // 单个 content 对象，尝试取 text 字段
            event
                .content
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_owned()
        }
        _ => String::new(),
    }
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_transcript(events: &[Value]) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        for ev in events {
            writeln!(f, "{}", serde_json::to_string(ev).unwrap()).unwrap();
        }
        f
    }

    #[test]
    fn no_thesis_signals_returns_false() {
        let f = write_transcript(&[
            serde_json::json!({ "type": "message", "role": "user", "content": "lint the code", "timestamp": "2026-05-17T10:00:00Z" }),
            serde_json::json!({ "type": "message", "role": "assistant", "content": "Done." }),
        ]);
        let summary = parse(f.path()).unwrap();
        assert!(!summary.is_thesis_domain);
        assert!(summary.session_start.is_some());
    }

    #[test]
    fn mcp_thesis_tool_call_detected() {
        let f = write_transcript(&[
            serde_json::json!({ "type": "message", "role": "user", "content": "init project", "timestamp": "2026-05-17T10:00:00Z" }),
            serde_json::json!({ "type": "tool_use", "content": { "name": "mcp__thesis__init" } }),
        ]);
        let summary = parse(f.path()).unwrap();
        assert!(summary.is_thesis_domain);
    }

    #[test]
    fn user_message_with_slash_thesis_detected() {
        let f = write_transcript(&[
            serde_json::json!({ "type": "message", "role": "user", "content": "/thesis write chapter 1", "timestamp": "2026-05-17T10:00:00Z" }),
        ]);
        let summary = parse(f.path()).unwrap();
        assert!(summary.is_thesis_domain);
    }

    #[test]
    fn user_message_with_chinese_keyword_detected() {
        let f = write_transcript(&[
            serde_json::json!({ "type": "message", "role": "user", "content": "帮我写论文", "timestamp": "2026-05-17T09:00:00Z" }),
        ]);
        let summary = parse(f.path()).unwrap();
        assert!(summary.is_thesis_domain);
    }

    #[test]
    fn session_start_from_first_event_timestamp() {
        let f = write_transcript(&[
            serde_json::json!({ "type": "message", "role": "user", "content": "hi", "timestamp": "2026-05-17T08:30:00Z" }),
            serde_json::json!({ "type": "message", "role": "user", "content": "/thesis", "timestamp": "2026-05-17T09:00:00Z" }),
        ]);
        let summary = parse(f.path()).unwrap();
        // session_start 应取第一行的时间戳，不是第二行
        let start = summary.session_start.unwrap();
        assert_eq!(start.to_rfc3339(), "2026-05-17T08:30:00+00:00");
        assert!(summary.is_thesis_domain);
    }

    #[test]
    fn empty_transcript_returns_false() {
        let f = NamedTempFile::new().unwrap();
        let summary = parse(f.path()).unwrap();
        assert!(!summary.is_thesis_domain);
        assert!(summary.session_start.is_none());
    }

    #[test]
    fn nonexistent_transcript_returns_false() {
        let summary = parse(Path::new("/nonexistent/transcript.jsonl")).unwrap();
        assert!(!summary.is_thesis_domain);
    }
}
