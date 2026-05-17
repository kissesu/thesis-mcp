//! @file pre_tool_use.rs
//! @description PreToolUse hook：拦截绕过 MCP 直接写 docx 的工具调用
//!
//! 拦截规则来源：HC-11, HC-13, HC-26
//!
//! 放行规则（白名单）：
//! - 工具名不在 {Write, Edit, MultiEdit, NotebookEdit, Bash, Agent} 之内 → 直接放行
//! - file_path 不匹配 *.docx → 放行
//! - Bash command 含白名单路径 thesis_docx_audit.py → 放行
//!
//! @author Atlas.oi
//! @date 2026-05-17

use std::io::Read;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

// ============================================================
// stdin JSON 结构
// ============================================================

/// CC PreToolUse hook 传入的 JSON 结构（只反序列化我们关心的字段）。
#[derive(Debug, Deserialize)]
pub struct HookInput {
    pub tool_name: String,
    pub tool_input: Value,
}

// ============================================================
// 正则：Bash 命令危险模式（HC-26）
//
// 匹配逻辑：
// - `python.*\.docx\b`  → python 脚本操作 docx
// - `node.*\.docx\b`    → node 脚本操作 docx
// - `zip\s+.*\.docx\b`  → zip 构造 docx（HC-13 字节级构造）
// - `cp\s+.*\.docx\b`   → cp 复制 docx
// - `mv\s+.*\.docx\b`   → mv 移动 docx
// - `libreoffice.*--convert` → LibreOffice 转换
// - `osascript.*[Ww]ord`    → AppleScript 操纵 Word（macOS）
// - `printf.*>.*\.docx\b`   → printf 字节写入 docx（HC-13）
// ============================================================
static BASH_BLOCK_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        // python 脚本操作 docx（含空格分隔参数）
        Regex::new(r"(?i)python[23]?\s+\S+.*\.docx\b").unwrap(),
        // node / nodejs 操作 docx
        Regex::new(r"(?i)node\b.*\.docx\b").unwrap(),
        // zip 构造 docx 包（HC-13 字节级）
        Regex::new(r"(?i)\bzip\b\s+.*\.docx\b").unwrap(),
        // cp / mv 操作 docx 文件
        Regex::new(r"(?i)\bcp\b\s+.*\.docx\b").unwrap(),
        Regex::new(r"(?i)\bmv\b\s+.*\.docx\b").unwrap(),
        // libreoffice 转换命令（转出 docx）
        Regex::new(r"(?i)\blibreoffice\b.*--convert").unwrap(),
        // macOS AppleScript 操纵 Word
        Regex::new(r"(?i)\bosascript\b.*[Ww]ord").unwrap(),
        // printf / echo 字节流重定向到 docx（HC-13）
        Regex::new(r"(?i)\b(printf|echo)\b.*>.*\.docx\b").unwrap(),
    ]
});

/// Bash 白名单文件名（argv 级别匹配，HC-26 白名单条款）。
///
/// 白名单原则：只放行 thesis_docx_audit.py 本身的诊断调用。
/// 检查方式：用 shell-words 解析命令为 argv token 列表，
/// 对注释符 `#` 前的每个 token 取 basename 与白名单比对。
/// 简单子串匹配已废弃——避免 `python evil.py # thesis_docx_audit.py` 注释注入绕过。
const BASH_WHITELIST_BASENAMES: &[&str] = &["thesis_docx_audit.py"];

/// Agent 工具 prompt 中的 thesis 域关键词（HC-11）。
///
/// 子 agent 不继承 thesis skill，不触发父会话 hook，必须在父会话拦截。
///
/// 注意：\b 是 ASCII word-boundary，对中文字符无效（中文字符不属于 \w）。
/// 中文关键词（论文、毕业论文）直接做子串匹配（无需 \b）。
/// ASCII 关键词（thesis、docx）用 \b 前后双锚保证单词边界。
/// `word\s*文档`：\b 只加在 word 前（`\bword`），不加尾部——因为紧跟的中文字符
/// 也不属于 \w，\b 在 ASCII-CJK 交界处不成立，无法用于尾部锚定。
/// 前置 `\b` 已足够阻止 "forward文档"/"password文档" 的误触发。
static AGENT_BLOCK_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        // ASCII 关键词：用 \b 保证单词边界（thesis/docx 双锚；word 只加前置锚）
        Regex::new(r"(?i)\b(thesis|docx)\b").unwrap(),
        // word\s*文档：前置 \b 防止 "forward文档" 误触发，尾部无锚（CJK 无 \b）
        Regex::new(r"(?i)\bword\s*文档").unwrap(),
        // 纯中文关键词：直接子串匹配（无 \b 边界概念）
        Regex::new(r"论文|毕业论文").unwrap(),
    ]
});

// ============================================================
// 主逻辑
// ============================================================

/// PreToolUse hook 入口，返回 exit code（0 = 放行，2 = 拦截）。
pub fn run() -> i32 {
    // 从 stdin 读取 CC 传入的 JSON
    let input = match read_stdin_json() {
        Ok(v) => v,
        Err(e) => {
            // stdin 读取/解析失败：安全起见放行（非 thesis 场景不阻断）
            eprintln!("[thesis-hook/pre] stdin 解析失败，放行: {e}");
            return 0;
        }
    };

    match check(&input) {
        Decision::Block(reason) => {
            eprintln!("{reason}");
            2
        }
        Decision::Allow => 0,
    }
}

/// 拦截决策结果。
#[derive(Debug, PartialEq)]
pub enum Decision {
    Block(String),
    Allow,
}

/// 核心拦截规则检查，对外暴露供单元测试使用。
pub fn check(input: &HookInput) -> Decision {
    match input.tool_name.as_str() {
        // Write / Edit / MultiEdit / NotebookEdit：只要 file_path 匹配 *.docx 就拦截
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => check_file_write(input),
        // Bash：解析 command，命中危险模式且不在白名单 → 拦截
        "Bash" => check_bash(input),
        // Agent：prompt 含 thesis 域关键词 → 拦截（HC-11）
        "Agent" => check_agent(input),
        // 其他工具：直接放行
        _ => Decision::Allow,
    }
}

// ============================================================
// 各工具类型的检查函数
// ============================================================

/// 检查文件写入类工具（Write / Edit / MultiEdit / NotebookEdit）。
fn check_file_write(input: &HookInput) -> Decision {
    let Some(file_path) = extract_file_path(&input.tool_input) else {
        return Decision::Allow; // 无 file_path 字段，放行
    };

    if is_docx_path(&file_path) {
        Decision::Block(format!(
            "[thesis-hook] 拦截：禁止直接用 {} 写入 docx 文件（{file_path}）。\n\
             请改用 mcp__thesis__write_section 或 mcp__thesis__revise 工具，\n\
             写入会经过审计并生成 manifest，确保 TOCTOU 保护。",
            input.tool_name
        ))
    } else {
        Decision::Allow
    }
}

/// 检查 Bash 命令（HC-26）。
fn check_bash(input: &HookInput) -> Decision {
    let Some(command) = input
        .tool_input
        .get("command")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned)
    else {
        return Decision::Allow;
    };

    // 白名单优先：命令的 argv token（注释之前）basename 命中白名单 → 放行（诊断 audit 场景）
    // 使用 shell-words 解析处理引号与转义，parse 失败则关闭白名单（fail-closed）。
    if is_bash_whitelisted(&command) {
        return Decision::Allow;
    }

    // 检查危险模式
    for pattern in BASH_BLOCK_PATTERNS.iter() {
        if pattern.is_match(&command) {
            return Decision::Block(format!(
                "[thesis-hook] 拦截：Bash 命令含禁止的 docx 写入模式（HC-26）。\n\
                 命令：{command}\n\
                 匹配模式：{pattern}\n\
                 请改用 mcp__thesis__write_section 或 mcp__thesis__revise 工具。"
            ));
        }
    }

    Decision::Allow
}

/// 判断 Bash 命令是否命中白名单（argv 级别，防注释注入绕过）。
///
/// 算法：
/// 1. 用 shell-words 将命令解析为 token 列表（处理引号/转义）。
///    解析失败 → fail-closed，返回 false（不放行）。
/// 2. 跳过 `#` 开头的 token 及其后所有 token（shell 注释部分）。
/// 3. 对剩余 token 取 Path basename，与 BASH_WHITELIST_BASENAMES 比对。
fn is_bash_whitelisted(command: &str) -> bool {
    // shell-words 解析失败（不平衡引号等）时 fail-closed
    let Ok(tokens) = shell_words::split(command) else {
        return false;
    };

    // 截断到第一个 shell 注释 token（以 `#` 开头）
    let effective_tokens: Vec<&str> = tokens
        .iter()
        .map(String::as_str)
        .take_while(|t| !t.starts_with('#'))
        .collect();

    // 检查有效 token 的 basename 是否命中白名单
    for token in &effective_tokens {
        let basename = Path::new(token)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if BASH_WHITELIST_BASENAMES.contains(&basename) {
            return true;
        }
    }
    false
}

/// 检查 Agent 工具（HC-11）。
///
/// 子 agent 不继承 thesis skill、不触发父会话 hook，
/// 若 prompt 含 thesis 域关键词说明试图绕过门禁。
fn check_agent(input: &HookInput) -> Decision {
    // 优先检查 prompt 字段，其次检查 description
    let text = {
        let prompt = input
            .tool_input
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let desc = input
            .tool_input
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        format!("{prompt} {desc}")
    };

    let matched = AGENT_BLOCK_PATTERNS.iter().any(|p| p.is_match(&text));
    if matched {
        Decision::Block(
            "[thesis-hook] 拦截：Agent 工具 prompt 含 thesis 域关键词（HC-11）。\n\
             子 agent 不继承 thesis skill，不触发父会话 hook，不读 HARD-GATE。\n\
             请在当前会话中直接使用 mcp__thesis__* 工具完成论文操作。"
                .to_owned(),
        )
    } else {
        Decision::Allow
    }
}

// ============================================================
// 辅助函数
// ============================================================

/// 从 `tool_input` 中提取 `file_path` 字段。
fn extract_file_path(tool_input: &Value) -> Option<String> {
    tool_input
        .get("file_path")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned)
}

/// 判断路径是否以 `.docx` 结尾（大小写不敏感）。
fn is_docx_path(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".docx")
}

/// 从 stdin 读取完整内容并解析为 `HookInput`。
fn read_stdin_json() -> Result<HookInput, anyhow::Error> {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    Ok(serde_json::from_str(&buf)?)
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json::json;

    /// 构造 HookInput 的测试辅助函数。
    pub fn make_input(tool_name: &str, tool_input: Value) -> HookInput {
        HookInput {
            tool_name: tool_name.to_owned(),
            tool_input,
        }
    }

    // ---- Write / Edit / MultiEdit / NotebookEdit ----

    #[test]
    fn blocks_write_docx() {
        // HC-26: Write {file_path: "*.docx"} 必须被拦截
        let input = make_input(
            "Write",
            json!({ "file_path": "/x/thesis.docx", "content": "hi" }),
        );
        let d = check(&input);
        assert!(matches!(d, Decision::Block(_)), "Write docx 应被拦截");
        if let Decision::Block(msg) = d {
            assert!(
                msg.contains("mcp__thesis__write_section"),
                "错误消息应包含 mcp__thesis__write_section，实际: {msg}"
            );
        }
    }

    #[test]
    fn blocks_edit_docx() {
        let input = make_input(
            "Edit",
            json!({ "file_path": "/docs/report.docx", "old_string": "a", "new_string": "b" }),
        );
        assert!(matches!(check(&input), Decision::Block(_)));
    }

    #[test]
    fn blocks_multiedit_docx() {
        let input = make_input("MultiEdit", json!({ "file_path": "/tmp/x.DOCX" }));
        assert!(
            matches!(check(&input), Decision::Block(_)),
            "大写 .DOCX 也应拦截"
        );
    }

    #[test]
    fn blocks_notebookedit_docx() {
        let input = make_input(
            "NotebookEdit",
            json!({ "file_path": "some/path/notebook.docx" }),
        );
        assert!(matches!(check(&input), Decision::Block(_)));
    }

    #[test]
    fn allows_write_to_md() {
        // 写 .md 文件不应被拦截
        let input = make_input(
            "Write",
            json!({ "file_path": "/x/notes.md", "content": "hello" }),
        );
        assert_eq!(check(&input), Decision::Allow);
    }

    #[test]
    fn allows_write_to_rs() {
        let input = make_input(
            "Write",
            json!({ "file_path": "/src/main.rs", "content": "fn main(){}" }),
        );
        assert_eq!(check(&input), Decision::Allow);
    }

    // ---- Bash ----

    #[test]
    fn blocks_bash_python_docx() {
        // HC-26: python 脚本操作 docx
        let input = make_input(
            "Bash",
            json!({ "command": "python convert.py thesis.docx" }),
        );
        assert!(matches!(check(&input), Decision::Block(_)));
    }

    #[test]
    fn blocks_bash_python3_docx() {
        let input = make_input("Bash", json!({ "command": "python3 gen.py output.docx" }));
        assert!(matches!(check(&input), Decision::Block(_)));
    }

    #[test]
    fn blocks_bash_node_docx() {
        let input = make_input(
            "Bash",
            json!({ "command": "node write_docx.js thesis.docx" }),
        );
        assert!(matches!(check(&input), Decision::Block(_)));
    }

    #[test]
    fn blocks_bash_zip_docx() {
        // HC-13: zip 字节级构造 docx
        let input = make_input("Bash", json!({ "command": "zip output.docx _rels/ word/" }));
        assert!(matches!(check(&input), Decision::Block(_)));
    }

    #[test]
    fn blocks_bash_cp_docx() {
        let input = make_input("Bash", json!({ "command": "cp backup.docx thesis.docx" }));
        assert!(matches!(check(&input), Decision::Block(_)));
    }

    #[test]
    fn blocks_bash_mv_docx() {
        let input = make_input("Bash", json!({ "command": "mv tmp.docx docs/thesis.docx" }));
        assert!(matches!(check(&input), Decision::Block(_)));
    }

    #[test]
    fn blocks_bash_libreoffice_convert() {
        let input = make_input(
            "Bash",
            json!({ "command": "libreoffice --headless --convert-to docx input.odt" }),
        );
        assert!(matches!(check(&input), Decision::Block(_)));
    }

    #[test]
    fn blocks_bash_osascript_word() {
        let input = make_input(
            "Bash",
            json!({ "command": "osascript -e 'tell application \"Microsoft Word\" to save'" }),
        );
        assert!(matches!(check(&input), Decision::Block(_)));
    }

    #[test]
    fn blocks_bash_printf_docx() {
        // HC-13: printf 字节流写入 docx
        let input = make_input(
            "Bash",
            json!({ "command": "printf '\\x50\\x4b...' > thesis.docx" }),
        );
        assert!(matches!(check(&input), Decision::Block(_)));
    }

    #[test]
    fn allows_innocent_bash() {
        // 普通 cargo build 不应被拦截
        let input = make_input("Bash", json!({ "command": "cargo build" }));
        assert_eq!(check(&input), Decision::Allow);
    }

    #[test]
    fn allows_bash_audit_script_whitelist() {
        // 白名单：运行 thesis_docx_audit.py 本身是合法的诊断调用
        let input = make_input(
            "Bash",
            json!({ "command": "python thesis_docx_audit.py docs/thesis.docx" }),
        );
        assert_eq!(
            check(&input),
            Decision::Allow,
            "thesis_docx_audit.py 应在白名单内放行"
        );
    }

    #[test]
    fn allows_bash_audit_script_with_absolute_path() {
        // 白名单：带绝对路径的合法诊断调用，basename 应被识别
        let input = make_input(
            "Bash",
            json!({ "command": "python /path/to/thesis_docx_audit.py thesis.docx" }),
        );
        assert_eq!(
            check(&input),
            Decision::Allow,
            "绝对路径 thesis_docx_audit.py 应在白名单内放行"
        );
    }

    #[test]
    fn blocks_bash_comment_injection_bypass() {
        // 安全修复验证：注释注入不得绕过白名单检查（Issue 1）
        // `# thesis_docx_audit.py` 属于 shell 注释，不是合法的 argv token
        let input = make_input(
            "Bash",
            json!({ "command": "python evil.py docx.docx # thesis_docx_audit.py" }),
        );
        assert!(
            matches!(check(&input), Decision::Block(_)),
            "注释注入绕过白名单应被拦截（exit 2）"
        );
    }

    #[test]
    fn allows_bash_git_operations() {
        let input = make_input("Bash", json!({ "command": "git status" }));
        assert_eq!(check(&input), Decision::Allow);
    }

    // ---- Agent ----

    #[test]
    fn blocks_agent_with_thesis_keyword() {
        // HC-11: subagent 含 thesis 关键词
        let input = make_input(
            "Agent",
            json!({ "prompt": "go write the thesis chapter 2" }),
        );
        assert!(matches!(check(&input), Decision::Block(_)));
    }

    #[test]
    fn blocks_agent_with_chinese_thesis_keyword() {
        let input = make_input("Agent", json!({ "prompt": "帮我写论文第三章" }));
        assert!(matches!(check(&input), Decision::Block(_)));
    }

    #[test]
    fn blocks_agent_with_docx_keyword() {
        let input = make_input(
            "Agent",
            json!({ "prompt": "edit the docx file and add a table" }),
        );
        assert!(matches!(check(&input), Decision::Block(_)));
    }

    #[test]
    fn allows_agent_unrelated() {
        // 与论文无关的 agent 任务应放行
        let input = make_input(
            "Agent",
            json!({ "prompt": "lint the Go code and fix errors" }),
        );
        assert_eq!(check(&input), Decision::Allow);
    }

    #[test]
    fn allows_unknown_tool() {
        // 未知工具名直接放行
        let input = make_input("ReadFile", json!({ "file_path": "/x/thesis.docx" }));
        assert_eq!(check(&input), Decision::Allow);
    }

    // ---- Agent word\s*文档 正则边界测试（Issue 2 修复验证）----

    /// 用于 AGENT_BLOCK_PATTERNS 的纯匹配辅助，不走工具名分发
    fn matches_agent_block(text: &str) -> bool {
        AGENT_BLOCK_PATTERNS.iter().any(|p| p.is_match(text))
    }

    #[test]
    fn agent_word_文档_strict() {
        // "用 word 文档写作业" 含独立 word → 应拦截
        assert!(
            matches_agent_block("用 word 文档写作业"),
            "含 'word 文档' 应拦截"
        );
        // "forward文档"：word 出现在 forward 内部，无单词边界 → 不应拦截
        assert!(
            !matches_agent_block("forward文档"),
            "'forward文档' 不应误拦截（word 在单词内部）"
        );
        // "password文档"：同上
        assert!(
            !matches_agent_block("password文档"),
            "'password文档' 不应误拦截（word 在单词内部）"
        );
        // "Word文档"（大写）→ 应拦截（(?i) 不区分大小写）
        assert!(
            matches_agent_block("Word文档"),
            "'Word文档' 应拦截（大小写不敏感）"
        );
    }
}
