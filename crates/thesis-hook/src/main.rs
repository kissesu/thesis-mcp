//! @file main.rs
//! @description thesis-hook 主入口：clap 子命令分发到 pre_tool_use / post_tool_use / stop
//! @author Atlas.oi
//! @date 2026-05-17

mod post_tool_use;
mod pre_tool_use;
mod stop;
mod transcript;

use clap::{Parser, Subcommand};

// ============================================================
// CLI 定义
// ============================================================

/// thesis-hook — Claude Code hook 二进制，强制 thesis 写入只走 MCP 工具。
///
/// CC settings.json 配置示例：
/// ```json
/// { "hooks": {
///     "PreToolUse":  [{ "command": "thesis-hook pre-tool-use"  }],
///     "PostToolUse": [{ "command": "thesis-hook post-tool-use" }],
///     "Stop":        [{ "command": "thesis-hook stop"          }]
///   }
/// }
/// ```
///
/// exit 0 = 放行；exit 2 = 拦截（stderr 消息展示给用户和 Claude）。
#[derive(Parser)]
#[command(name = "thesis-hook", about = "thesis-mcp CC hook binary")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// PreToolUse：拦截直接写 docx 的工具调用
    #[command(name = "pre-tool-use")]
    PreToolUse,

    /// PostToolUse：非 MCP 路径写入 docx 后的兜底审计
    #[command(name = "post-tool-use")]
    PostToolUse,

    /// Stop：会话结束时的 TOCTOU 扫描 + mtime 孤儿 docx 检测
    #[command(name = "stop")]
    Stop,
}

fn main() {
    let cli = Cli::parse();

    // 根据子命令分发，各子命令负责读 stdin / exit code
    let exit_code = match cli.command {
        Command::PreToolUse => pre_tool_use::run(),
        Command::PostToolUse => post_tool_use::run(),
        Command::Stop => stop::run(),
    };

    std::process::exit(exit_code);
}
