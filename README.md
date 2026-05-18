# thesis-mcp

中文学术论文写作的"强制层"。在 Claude Code 中给 `/thesis` 技能加上一组守门规则，让 Claude 写论文时**没法跳步骤、伪造审计结果、或绕过格式检查直写 docx**。

核心机制：MCP 工具垄断 docx 写入入口 + 钩子拦截 Bash/Write 直写路径 + manifest 协议防 TOCTOU 与 subagent 委派绕过。每一次 docx 写入都必须经过审计流程。

---

## 安装（macOS / Linux）

### 1. 下载

从 https://github.com/kissesu/thesis-mcp/releases/latest 选与你平台匹配的 `.tar.gz`：

| 平台 | 包名 |
|---|---|
| macOS Apple Silicon | `thesis-mcp-X.Y.Z-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `thesis-mcp-X.Y.Z-x86_64-apple-darwin.tar.gz` |
| Linux x86_64 | `thesis-mcp-X.Y.Z-x86_64-unknown-linux-gnu.tar.gz` |
| Linux arm64 | `thesis-mcp-X.Y.Z-aarch64-unknown-linux-gnu.tar.gz` |
| Windows x64 | `thesis-mcp-X.Y.Z-x86_64-pc-windows-msvc.zip`（用 git-bash 或 WSL 执行下一步） |

### 2. 解压（重要：必须建子目录后再解压）

包根是 `./`，直接 `tar -xzf -C ~/Downloads/` 会把 `bin/ skills/ install.sh README.md` **平铺到父目录**污染。正确做法：

```bash
mkdir -p ~/.local/opt/thesis-mcp-X.Y.Z
tar -xzf ~/Downloads/thesis-mcp-X.Y.Z-<平台>.tar.gz -C ~/.local/opt/thesis-mcp-X.Y.Z/
```

放哪个目录由你定，约定俗成走 `~/.local/opt/` 或 `~/Applications/`。

### 3. 跑安装脚本

```bash
bash ~/.local/opt/thesis-mcp-X.Y.Z/install.sh
```

脚本会自动完成 6 件事：

1. 自检模式（含 `bin/` = 预编译包模式，含 `Cargo.toml` = 源码模式）
2. 校验二进制存在
3. （仅 macOS）清除 `com.apple.quarantine` 属性，避免系统弹"无法验证开发者"框
4. 在 `~/.claude/hooks/` 建软链指向解压目录的二进制
5. 复制技能到 `~/.claude/skills/thesis/`
6. 写入 `~/.claude.json` 的 MCP 服务条目 + `~/.claude/settings.json` 的 3 条钩子（工具调用前 / 会话结束 / 工具调用后）

写入前自动备份两个 `.json` 文件到同目录 `.bak.<时间戳>`。

依赖：`jq`（macOS `brew install jq` / Linux 包管理器装）。

### 4. 验证

重启 Claude Code，然后：

- 输入 `/thesis` → 应触发技能
- 让 Claude 尝试 `Write test.docx` → 应被工具调用前钩子阻断（退出码 2，提示走 `mcp__thesis__write_section`）

---

## 卸载（完整清理）

四步走，缺一不可：

### A. 取消所有注册

```bash
bash ~/.local/opt/thesis-mcp-X.Y.Z/install.sh --uninstall
```

清掉：`~/.claude/hooks/thesis-*` 软链、`~/.claude/skills/thesis/` 技能、`~/.claude.json` 的 `mcpServers["thesis"]`、`settings.json` 的钩子条目。**会写入新备份**。

### B. 删物理目录

```bash
rm -rf ~/.local/opt/thesis-mcp-X.Y.Z/
```

### C. 清备份文件

```bash
rm -f ~/.claude.json.bak.* ~/.claude/settings.json.bak.*
```

确认安装稳定后再删；想保留回滚退路就跳过这步。

### D. 检查 JS 旧版残留（重要）

本项目是 Rust 重写。早期 JS 实现 `~/.claude/hooks/thesis-stop-guard.js` + 配套 `thesis_docx_audit.py` 可能还在。`--uninstall` 当前的 jq 过滤只命中含字串 "thesis-hook" 的钩子条目，**漏删** "thesis-stop-guard.js" 这种含 "thesis" 但不含 "thesis-hook" 的旧条目：

```bash
ls ~/.claude/hooks/ | grep thesis     # 应为空
rm -f ~/.claude/hooks/thesis-stop-guard.js ~/.claude/hooks/thesis_docx_audit.py
```

### 残留全栈校验

跑完应该全部为 0：

```bash
ls ~/.claude/hooks/ | grep -ic thesis
ls ~/.claude/skills/ | grep -ic thesis
jq '.mcpServers.thesis // "removed"' ~/.claude.json
jq '[.hooks | to_entries[] | .value[]? | .hooks[]? | select(.command | tostring | test("thesis"))] | length' ~/.claude/settings.json
ls ~/.local/opt/ | grep -ic thesis
```

---

## macOS 用户：隔离属性踩坑

浏览器或 `curl` 从 GitHub 下载的 `.tar.gz` 会被加 `com.apple.quarantine` 扩展属性，解压时**文件和父目录都继承**这个属性。Claude Code 启动子进程跑 MCP 服务时，系统看门人（Gatekeeper）会拦截弹"无法验证开发者"授权框。ad-hoc 签名解决不了。

`install.sh` 步骤 3 会自动清除。如果手动验证：

```bash
xattr -lr ~/.local/opt/thesis-mcp-X.Y.Z/ | grep -c com.apple.quarantine   # 期望 0
```

如还有残留，手动清：

```bash
xattr -dr com.apple.quarantine ~/.local/opt/thesis-mcp-X.Y.Z/
```

---

## 让 Claude 帮你装

如果你正在和 Claude Code 对话且不想自己跑命令，可以把这段话复制给它：

> 帮我装 thesis-mcp。tarball 在 `<填路径>`。完整流程：建 `~/.local/opt/thesis-mcp-X.Y.Z/` 子目录、`tar -xzf` 解压进去、跑 `install.sh`、跑完后校验 quarantine 残留为 0、确认软链 + mcpServers + settings.json 钩子都注册到位。

Claude 应该会按 4 步执行 + 跑校验命令。如果跑出问题，让它读本文件的"卸载"节走完整回滚再重试。

---

## 开发者本地构建

只在以下场景需要：
- 改 Rust 源码后想本地测试（不走 release 包）
- 仓库尚未发布版本时调试
- 网络受限拉不到 release 包

```bash
git clone https://github.com/kissesu/thesis-mcp.git
cd thesis-mcp
bash scripts/install.sh
```

`install.sh` 自动识别项目根有 `Cargo.toml` → 源码模式，跑 `cargo build --release --workspace` 后建软链。后续步骤与预编译包模式相同。

依赖：Rust 1.95+ / jq / Claude Code。

### 卸载本地构建

```bash
bash scripts/install.sh --uninstall
bash scripts/install.sh --uninstall --restore-backup   # 同时还原 settings.json 备份
```

---

## 调试

### 看 MCP 服务日志

Claude Code 把 MCP 服务的 stderr 写到会话日志：

```bash
ls ~/.claude/projects/                            # 找当前项目目录
tail -f ~/.claude/projects/<项目哈希>/sessions/<会话 id>.jsonl \
  | jq 'select(.type=="mcp_stderr")'
```

### 直接测试钩子

测试 docx 写入被阻断（应 exit 2）：

```bash
echo '{"tool_name":"Write","tool_input":{"file_path":"chapter1.docx","content":""}}' \
  | ~/.claude/hooks/thesis-hook pre-tool-use
echo "exit: $?"
```

测试普通文件不被阻断（应 exit 0）：

```bash
echo '{"tool_name":"Write","tool_input":{"file_path":"src/main.rs","content":""}}' \
  | ~/.claude/hooks/thesis-hook pre-tool-use
echo "exit: $?"
```

注意：测试命令的 bash 字串里**不要同时出现 `>` 和字面 `.docx`**。HC-26 规则的正则 `echo.*>.*\.docx` 会跨命令贪婪匹配（包括 `2>&1` 的 `>`），把你的测试命令本身拦掉。绕过：分两次跑、或用 `cat <<<` 提供 stdin、或先把 JSON 写到临时文件再 `cat`。

### 看审计日志

```bash
cat <项目目录>/.thesis/audit-log.jsonl | jq '.'
```

### 不修改任何文件的演练

```bash
HOME=$(mktemp -d) bash scripts/install.sh --dry-run
```

---

## 项目架构（给维护者）

```
thesis-mcp/
├── crates/
│   ├── thesis-types        # 共享类型：RuleId / Severity / Manifest / WriteOp / AuditResult
│   ├── thesis-audit        # 核心审计库：OOXML 全包扫描
│   ├── thesis-manifest     # manifest 协议：sha256 + mtime 锁定 + audit-log.jsonl
│   ├── thesis-mcp-server   # MCP stdio 服务：init / write_section / revise / audit 四个工具
│   └── thesis-hook         # clap 二进制：pre-tool-use / post-tool-use / stop 子命令
└── scripts/
    ├── install.sh          # 双模式安装：源码模式 cargo build / 预编译包模式直接软链
    └── ci-test.sh          # 质量门禁：fmt + clippy + build + test + 对抗测试摘要
```

---

## HC 覆盖现状（给维护者）

| 编号 | 描述 | 状态 |
|---|---|---|
| HC-4  | thesis 域错误不静默通过 | 已实现（fail-closed） |
| HC-5  | bypassPermissions 下 Bash 阻断 | 已实现 |
| HC-11 | Agent 委派绕过阻断 | 已实现（Agent 工具 prompt 关键词检测） |
| HC-13 | `printf+zip` 字节构造阻断 | 已实现 |
| HC-22 | 防御层文件自保护 | 已实现 |
| HC-23 | TOCTOU 防护 | 已实现（manifest 与磁盘对比校验） |
| HC-25 | 审计失败一律拒绝 | 已实现 |
| HC-26 | Bash docx 写入正则阻断 | 已实现 |
| HC-27/28 | MCP 服务健康检查 | 已实现 |
| HC-29 | 全文件 mtime 扫描 | 已实现（会话结束钩子） |
| HC-30 | manifest 锁定本轮目标 | 已实现 |
| HC-31 | 完整 OOXML 包审计 | 已实现 |
| HC-32 | 自检表钩子注入 | **延后** — 需工具调用后钩子 + MCP 服务跨进程协同 |

**不适用**：HC-1 / HC-2（已由 Rust 钩子替代旧 JS 实现）、HC-17（Claude Code 架构层问题，非 Rust 层职责）。

---

## License

MIT
