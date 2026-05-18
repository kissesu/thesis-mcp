# thesis-mcp

## 项目简介

Rust 重写的 `/thesis` skill 强制层。通过"白名单收口"保证 Claude 无法跳步骤、伪造审计结果或绕过格式检查直写 docx。

核心目标：用 manifest 协议阻断 TOCTOU 竞争写入、subagent 委派绕过和 Bash 直写路径，确保每一次 docx 写入都必须经过 MCP 工具的审计流程。

---

## 架构

```
thesis-mcp/
├── crates/
│   ├── thesis-types        # 共享类型层：RuleId / Severity / Manifest / WriteOp / AuditResult
│   ├── thesis-audit        # 核心审计库：OOXML 全包扫描（document / tables / textbox /
│   │                       #   headers_footers / comments / footnotes / tracked_changes / styles）
│   ├── thesis-manifest     # manifest 协议：sha256 / mtime 锁定 + audit-log.jsonl 持久化
│   ├── thesis-mcp-server   # MCP stdio server：暴露 init / write_section / revise / audit 四个工具
│   └── thesis-hook         # clap 二进制：pre-tool-use / post-tool-use / stop 子命令，
│                           #   作为 Claude Code hook 拦截 Bash/Write 直写路径
└── scripts/
    ├── install.sh          # 一键安装：编译 → 软链 → 写入 settings.json + .claude.json
    └── ci-test.sh          # CI 质量门禁：fmt + clippy + build + test + 对抗测试摘要
```

---

## 快速开始

### 安装（推荐：Claude Code plugin）

```bash
claude plugin install kissesu/thesis-mcp
```

一行装完，自动完成：
1. 拉取 skill (`skills/thesis/`) 注入 system prompt
2. 注册 PreToolUse / Stop / PostToolUse hooks
3. 注册 MCP server (`thesis`)
4. **首次使用时**自动从 GitHub release 拉取当前平台的预编译二进制（macOS arm64/x64、Linux x64/arm64、Windows x64），无需本地 Rust 工具链

### 验证

```bash
# 重启 Claude Code 后，输入 /thesis 触发 skill
# 或尝试 Write test.docx，应被 PreToolUse hook 阻断
```

### 卸载

```bash
claude plugin uninstall thesis-mcp
```

---

## 开发者本地构建（dev fallback）

只在以下场景需要：
- 改 Rust 源码后想本地测试（不走 GitHub release）
- 仓库尚未发布 release 时调试
- 网络受限拉不到 release tarball

```bash
git clone https://github.com/kissesu/thesis-mcp.git
cd thesis-mcp
bash scripts/install.sh   # 编译 → 软链 → 改 settings.json
```

> **注意**：`scripts/install.sh` 是 plugin 系统之前的旧路径，仅保留作开发者本地构建用。普通用户应走 `claude plugin install`。

依赖：Rust 1.95+ / jq / Claude Code。

### 卸载本地构建

```bash
bash scripts/install.sh --uninstall
bash scripts/install.sh --uninstall --restore-backup   # 同时还原 settings.json 备份
```

---

## 调试

**查看 MCP server 日志**（CC 捕获 stdio server 的 stderr）：

```bash
# 找到当前项目的会话目录，实际路径因项目而异
ls ~/.claude/projects/
tail -f ~/.claude/projects/<project-hash>/sessions/<session-id>.jsonl \
  | jq 'select(.type=="mcp_stderr")'
```

**直接测试 PreToolUse hook**：

```bash
# 测试 Write docx 是否被阻断（应 exit 2）
echo '{"tool_name":"Write","tool_input":{"file_path":"thesis/chapter1.docx","content":""}}' \
  | ~/.claude/hooks/thesis-hook pre-tool-use
echo "exit: $?"

# 测试普通文件写入（应 exit 0，不阻断）
echo '{"tool_name":"Write","tool_input":{"file_path":"src/main.rs","content":""}}' \
  | ~/.claude/hooks/thesis-hook pre-tool-use
echo "exit: $?"
```

**查看审计日志**：

```bash
cat <project-dir>/.thesis/audit-log.jsonl | jq '.'
```

**Dry-run 安装（不修改任何文件）**：

```bash
HOME=$(mktemp -d) bash scripts/install.sh --dry-run
```

---

## HC 覆盖表

| HC 编号 | 描述 | 状态 |
|---------|------|------|
| HC-5  | bypassPermissions 下 Bash 阻断 | 已实现（thesis-hook pre_tool_use） |
| HC-11 | Agent 委派绕过阻断 | 已实现（Agent 工具 prompt 关键词检测） |
| HC-13 | printf+zip 字节构造阻断 | 已实现（Bash 命令模式匹配） |
| HC-17 | 无差别 mtime 扫描 | 不适用 — 架构层（CC 自身 transcript 可见性问题，非 Rust 层职责）|
| HC-22 | 防御层文件自保护 | 已实现（阻断对 ~/.claude/hooks/thesis-* 的写入） |
| HC-23 | TOCTOU 防护 | 已实现（manifest verify_against_disk） |
| HC-4  | thesis 域错误不静默通过 | 已实现 — stop.rs（fail-closed，thesis 域内异常 exit 2）|
| HC-25 | fail-closed | 已实现（audit 失败/超时一律 exit 2） |
| HC-26 | Bash docx 写入阻断规则 | 已实现（pre_tool_use 命令模式列表） |
| HC-27/28 | MCP server 健康检查 | 已实现（health.rs） |
| HC-29 | mtime 无差别扫 | 已实现（Stop hook） |
| HC-30 | manifest 锁定本轮目标 | 已实现（thesis-manifest crate） |
| HC-31 | 完整 OOXML 包审计 | 已实现（thesis-audit 全 part 扫描） |
| HC-32 | 自检表 hook 注入 | **延后** — 需 PostToolUse + MCP server 跨进程协同（additionalContext 注入）|

**延后清单**：HC-32（self-check 表注入需 hook 与 MCP server 跨进程协同，本项目当前架构未实现该回路）

**不适用**：HC-1/HC-2/HC-17（HC-1/HC-2 已由 thesis-hook 替代旧 thesis-stop-guard.js；HC-17 为 CC 架构层问题，非 Rust 层职责）

---

## License

MIT
