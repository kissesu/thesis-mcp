//! @file main.rs
//! @description thesis-mcp-server 主入口：rmcp stdio MCP 服务，暴露 init / audit 工具
//! @author Atlas.oi
//! @date 2026-05-17

use rmcp::{
    ServiceExt,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content},
    tool, tool_router,
};

mod health;
mod tools;

use tools::{AuditParams, InitParams, StubAuditEngine, run_audit, run_init};

// ─── MCP 服务器结构体 ─────────────────────────────────────────────────────────

/// thesis-mcp MCP 服务器主体。
///
/// 业务流程：
/// 1. 启动时执行健康检查（health.rs）
/// 2. 通过 rmcp stdio 协议暴露工具给 Claude Code
/// 3. 每个工具调用路由到 tools/ 子模块
#[derive(Debug, Clone)]
struct ThesisMcpServer;

// ─── 工具路由 ──────────────────────────────────────────────────────────────────
// `tool_router` 宏为每个 #[tool] fn 生成 `{fn_name}_tool_attr()` 和路由入口，
// `server_handler` 标志同时生成 `ServerHandler` 的 list_tools / call_tool / get_info 实现。
// 因此不应再手动 `impl ServerHandler`。

#[tool_router(server_handler)]
impl ThesisMcpServer {
    /// 初始化论文项目目录，创建 .thesis/progress.md、outline.md、format-spec.md 骨架文件。
    #[tool(description = "初始化论文项目：在 thesis_root 下创建 .thesis/ 骨架文件")]
    async fn init(&self, Parameters(params): Parameters<InitParams>) -> CallToolResult {
        match run_init(&params) {
            Ok(output) => {
                let json = serde_json::to_string_pretty(&output)
                    .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}).to_string());
                CallToolResult::success(vec![Content::text(json)])
            }
            Err(e) => {
                tracing::error!("init 工具错误: {e}");
                CallToolResult::error(vec![Content::text(format!("init 失败: {e}"))])
            }
        }
    }

    /// 审计指定 docx 文件，检查格式规范符合性，返回 AuditResult JSON。
    #[tool(description = "审计 docx 文件格式规范符合性，返回结构化 AuditResult")]
    async fn audit(&self, Parameters(params): Parameters<AuditParams>) -> CallToolResult {
        let engine = StubAuditEngine;
        match run_audit(&engine, &params) {
            Ok(json) => CallToolResult::success(vec![Content::text(json)]),
            Err(e) => {
                tracing::error!("audit 工具错误: {e}");
                CallToolResult::error(vec![Content::text(format!("audit 失败: {e}"))])
            }
        }
    }

    // TODO L3.1: write_section 工具（L3.1 实现后在此添加）
    // TODO L3.1: revise 工具（L3.1 实现后在此添加）
}

// ─── 主函数 ───────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化结构化日志（输出到 stderr，不污染 MCP stdio 协议通道）
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_env("THESIS_LOG")
                .add_directive(tracing_subscriber::filter::LevelFilter::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .json()
        .init();

    tracing::info!("thesis-mcp-server 启动，版本 {}", env!("CARGO_PKG_VERSION"));

    // 健康检查：在当前工作目录下验证环境
    let cwd = std::env::current_dir()?;
    let report = health::HealthReport::check(&cwd);
    if report.all_ok {
        tracing::info!(
            "健康检查通过 ooxmlsdk_ok={} workdir_writable={}",
            report.ooxmlsdk_ok,
            report.workdir_writable
        );
    } else {
        tracing::warn!(
            "健康检查未完全通过 ooxmlsdk_ok={} workdir_writable={}，继续启动",
            report.ooxmlsdk_ok,
            report.workdir_writable
        );
    }

    // 启动 rmcp stdio MCP 服务（阻塞，等待 JSON-RPC 消息）
    tracing::info!("等待 MCP 客户端连接（stdio）…");
    ThesisMcpServer
        .serve(rmcp::transport::stdio())
        .await?
        .waiting()
        .await?;

    tracing::info!("thesis-mcp-server 已退出");
    Ok(())
}
