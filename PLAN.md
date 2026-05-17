# Team Plan: thesis-skill-enforcement (Rust MCP)

> 实施目录：`/Users/oi/CodeCoding/Code/自研项目/Skills/thesis-mcp`
> 研究文件：`~/.claude/team-plan/thesis-skill-enforcement-research.md`（32 条硬约束 + 6 软约束 + 风险 + 成功判据）
> Owner：用户在本目录重开会话推进实施
> 总工程量：14-21 人日（codex 实证支撑，含 ×1.5 风险缓冲）

---

## 概述

构建 Rust thesis MCP server，作为 `/thesis` skill 的**唯一 docx 写入入口**，配合 PreToolUse hook 收口 Bash/Write 直写路径，通过 manifest 协议防 TOCTOU 和 subagent 委派绕过。实现"白名单收口"控制 Claude 无法偷懒 / 跳步骤 / 伪 PASS。

---

## 技术栈（基于 3 轮 codex 实证终选）

| 角色 | 选型 | 版本 | 用途 |
|---|---|---|---|
| MCP 协议 | `rmcp` | 0.x 最新 | MCP server stdio JSON-RPC |
| OOXML typed model | `ooxmlsdk` | 0.6.1+ | docx package / parts / numbering / styles / headers / footers / typed read+write |
| Raw XML 流式 | `quick-xml` | 0.40.x | document.xml / textbox / tracked changes / 嵌套表格 cell pPr 的 token-preserving rewrite |
| DOM 备用 | `xot` | 最新 | 局部 subtree namespace-aware 重组 |
| XPath 诊断 | `xee-xpath` | 0.x | 测试断言 + 诊断查询（不进生产写回主路径） |
| zip 操作 | `zip` | 0.6.x | docx zip read/write + 未触碰 part byte-for-byte passthrough |
| 错误处理 | `thiserror` + `anyhow` | 最新 | 库层 thiserror / 应用层 anyhow |
| 异步 runtime | `tokio` | 1.x | MCP stdio + 文件 I/O |
| 序列化 | `serde` + `serde_json` | 最新 | manifest + audit log |
| 哈希 | `sha2` | 0.10.x | docx sha256 |
| CLI | `clap` | 4.x | 子命令（serve / audit / init / hook） |
| 测试 | `insta` + `tempfile` | 最新 | snapshot 测试 + 临时目录 fixture |
| Log | `tracing` + `tracing-subscriber` | 最新 | 结构化日志写 .thesis/audit-log.jsonl |

**明确不用**：
- ❌ `libxml` —— C 依赖复杂 + 与 ooxmlsdk 形成双 XML 运行时（codex 实证）
- ❌ `docx-rs` / `docx-rust` —— 现有 unknown elements 处理不全，ooxmlsdk 更新
- ❌ `serde-xml-rs` / `hard-xml` —— 不能承担 OOXML raw fallback

---

## cargo workspace 结构

```
thesis-mcp/
├── Cargo.toml                      # workspace root
├── PLAN.md                         # 本文件
├── README.md                       # 用户文档（最后阶段写）
├── crates/
│   ├── thesis-mcp-server/          # MCP server 主 binary（rmcp）
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── main.rs             # stdio MCP server entry
│   │   │   ├── tools/              # MCP tool 实现
│   │   │   │   ├── mod.rs
│   │   │   │   ├── init.rs         # mcp__thesis__init
│   │   │   │   ├── write_section.rs # mcp__thesis__write_section
│   │   │   │   ├── revise.rs       # mcp__thesis__revise
│   │   │   │   └── audit.rs        # mcp__thesis__audit
│   │   │   └── health.rs           # health check endpoint (HC-28)
│   │   └── tests/
│   ├── thesis-audit/               # audit 核心库（被 server + hook 共用）
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs              # 公共 API: audit_full / audit_section
│   │   │   ├── document.rs         # ooxmlsdk + quick-xml document.xml 扫描
│   │   │   ├── numbering.rs        # numbering.xml typed + lvlText 真验证（HC-8）
│   │   │   ├── tables.rs           # 嵌套表格 cell pPr 扫描（HC-7）
│   │   │   ├── textbox.rs          # textbox 内段落（HC-7）
│   │   │   ├── tracked_changes.rs  # w:ins/w:del + strike 残留（F.5.2）
│   │   │   ├── headers_footers.rs  # 页眉页脚扫描（HC-6）
│   │   │   ├── comments.rs         # 批注扫描（HC-6）
│   │   │   ├── footnotes.rs        # 脚注尾注扫描（HC-6）
│   │   │   ├── styles.rs           # styles.xml 字体/上标格式检查
│   │   │   ├── rules/              # G 系规则实现
│   │   │   │   ├── a_anti_ai.rs    # A 系（黑词 / em dash / CJK 间距）
│   │   │   │   ├── c_citation.rs   # C 系（[N] 上标 / 顺序）
│   │   │   │   ├── d_tables.rs     # D 系（cell pPr 清零）
│   │   │   │   ├── e_format.rs     # E 系（自动编号）
│   │   │   │   └── mod.rs
│   │   │   └── error.rs            # thiserror 定义
│   │   └── tests/
│   ├── thesis-manifest/            # manifest 协议 + 持久化
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs              # Manifest struct + read/write/verify
│   │   │   ├── schema.rs           # JSON schema 定义
│   │   │   └── store.rs            # .thesis/audit-log.jsonl 追加 + 查询
│   │   └── tests/
│   ├── thesis-hook/                # PreToolUse / Stop hook binary（替代 thesis-stop-guard.js）
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── main.rs             # clap 子命令分发
│   │   │   ├── pre_tool_use.rs     # 阻断 Bash/Write 直写 docx（HC-26）
│   │   │   ├── post_tool_use.rs    # 写完 docx 跑 audit + 注入 additionalContext
│   │   │   └── stop.rs             # 兜底扫 mtime + manifest 比对 + fail-closed
│   │   └── tests/
│   └── thesis-types/               # 共享类型（避免循环依赖）
│       ├── Cargo.toml
│       └── src/lib.rs              # Rule ID enum / Severity / Manifest schema
├── tests/
│   ├── fixtures/                   # 中文论文 docx fixture
│   │   ├── valid_simple.docx       # 干净基线
│   │   ├── violation_table_indent.docx     # D.9.1 cell pPr 违规
│   │   ├── violation_chapter_handnum.docx  # E.5.7 手打章节号
│   │   ├── violation_ref_handbracket.docx  # E.5.8 手打 [N]
│   │   ├── revision_strike_residual.docx   # F.5.2 strike 残留
│   │   ├── violation_textbox_blackword.docx # 文本框藏黑词
│   │   ├── violation_footer_blackword.docx  # 页脚藏黑词
│   │   ├── nested_table.docx       # 嵌套表格 cell
│   │   ├── tracked_changes.docx    # 修订模式
│   │   └── README.md               # fixture 说明
│   └── adversarial/                # 10 类对抗测试
│       ├── test_ast_obfuscation.rs       # __import__('doc'+'x') 绕过
│       ├── test_zip_byte_construct.rs    # printf+zip 字节构造
│       ├── test_subagent_delegation.rs   # Agent 工具委派
│       ├── test_fake_pass_string.rs      # 伪造 "脚本输出: 0 处"
│       ├── test_baseline_regression.rs   # 删违规段重生成
│       ├── test_short_response_bypass.rs # < 200 字阈值绕过
│       ├── test_background_delay_write.rs # 后台延迟写
│       ├── test_hidden_region.rs         # 隐藏页脚违规
│       ├── test_empty_numpr.rs           # 空 numPr 伪通过
│       └── test_self_modify_hook.rs      # 改 hook 自身绕过
├── scripts/
│   ├── install.sh                  # 把 binary 软链到 ~/.claude/hooks/
│   ├── ci-test.sh                  # CI 跑测
│   └── benchmark.sh                # 性能基准
└── docs/                           # 用户写论文用，不动
```

---

## 子任务列表（按 Layer 并行分组）

### Layer 1（基础设施，并行）

#### Task L1.1: cargo workspace 初始化
- **文件范围**: `Cargo.toml`（workspace root）/ `.gitignore` / `rustfmt.toml` / `clippy.toml`
- **依赖**: 无
- **实施步骤**:
  1. `cd /Users/oi/CodeCoding/Code/自研项目/Skills/thesis-mcp && git init`
  2. 创建 workspace `Cargo.toml`：`resolver = "2"`，`members = ["crates/*"]`
  3. 加 workspace dependencies（统一管理：rmcp / ooxmlsdk / quick-xml / xot / serde / tokio 等）
  4. 加 `.gitignore`：`target/` / `.thesis/` / `docs/*.docx`
  5. `rustfmt.toml`：`edition = "2024"` / `tab_spaces = 4`
  6. `clippy.toml`：开启 pedantic 严格度
- **验收标准**: `cargo check --workspace` 通过；`cargo fmt --all --check` 通过

#### Task L1.2: thesis-types 共享类型 crate
- **文件范围**: `crates/thesis-types/`
- **依赖**: L1.1
- **实施步骤**:
  1. `Rule ID enum`（A.1/A.5/A.6/A.7/A.9/C.1/C.2/D.9.1/D.9.2/E.5.7/E.5.8/F.5.1/F.5.2）
  2. `Severity enum`（Critical/Warning/Info）
  3. `Manifest struct`（docx_path: PathBuf / sha256: [u8;32] / mtime: SystemTime / op: WriteOp / rule_hits: HashMap<RuleId, usize> / audit_version: String / nonce: Uuid / session_id: String / turn_id: String）
  4. `WriteOp enum`（WriteSection / Revise / ExternalEdit）
  5. `AuditResult struct`（含 self_check_table: Vec<CheckRow>）
- **验收标准**: 所有 struct 实现 Serialize + Deserialize + Debug + Clone；`cargo test -p thesis-types` 通过

### Layer 2（核心组件，依赖 L1 全部完成）

#### Task L2.1: thesis-audit 核心库（最大子任务）
- **文件范围**: `crates/thesis-audit/`
- **依赖**: L1.2
- **实施步骤**:
  1. **document.rs**: 用 ooxmlsdk 加载 docx package，遍历 main document part；用 quick-xml NsReader 流式扫描 body 段落 + run
  2. **numbering.rs**: ooxmlsdk typed 拿 NumberingPart，遍历 num + abstractNum，建 `numId → lvlText[]` 映射；验证段落 numPr 指向的 lvlText 是否符合 E.5.7 章节号模式 (`%1.` / `%1.%2`) 或 E.5.8 参考文献模式 (`[%1]`)
  3. **tables.rs**: quick-xml 维护栈路径 `w:tbl/w:tr/w:tc/w:p/w:pPr`，递归处理嵌套表格；对每个 cell 段落验证 `pPr` 含 `firstLineChars="0" leftChars="0" left="0" firstLine="0"` 或同等
  4. **textbox.rs**: quick-xml 扫描 `w:drawing` / `v:textbox` / `mc:AlternateContent`，对内部段落跑同样的 A 系黑词扫描
  5. **tracked_changes.rs**: quick-xml 识别 `w:ins` / `w:del` / `w:moveFrom` / `w:moveTo`，按 RGB(0,0,255) 蓝色筛选；扫描 strike 残留 / 字体继承 / [N] 上标格式
  6. **headers_footers.rs**: ooxmlsdk 遍历所有 HeaderPart/FooterPart，对每个 part 跑 A 系扫描 + 字数计入页脚 budget
  7. **comments.rs / footnotes.rs**: 同上模式
  8. **styles.rs**: ooxmlsdk typed 拿 StylesPart，验证字体名 / 字号 / 颜色 / vertAlign 在 rPr 链中正确继承
  9. **rules/**: 每个 G 系规则独立 fn，统一签名 `fn check(doc: &Document, ctx: &AuditContext) -> Vec<Violation>`
  10. **error.rs**: thiserror 定义 AuditError（ParseError / IoError / SchemaViolation / TocTouViolation 等）
  11. **lib.rs**: 公共 API `audit_full(docx_path) -> AuditResult` 和 `audit_section(docx_path, section_id) -> AuditResult`
- **验收标准**:
  - `cargo test -p thesis-audit` 全部测试通过
  - 每个 rules/*.rs 含至少 2 个 fixture 测试（命中 + 不命中）
  - 嵌套表格 / textbox / tracked_changes / footnotes 4 个独立 fixture 都能正确识别违规

#### Task L2.2: thesis-manifest crate
- **文件范围**: `crates/thesis-manifest/`
- **依赖**: L1.2
- **实施步骤**:
  1. **schema.rs**: 定义 Manifest JSON schema（与 thesis-types::Manifest 对应）
  2. **lib.rs**: `Manifest::new(docx_path, op) -> Self` / `Manifest::compute_sha256()` / `Manifest::write_to(path)` / `Manifest::verify_against_disk()`（返回 TocTouViolation 如果 mtime > manifest 记录）
  3. **store.rs**: `AuditLog::append(manifest)` 写 `.thesis/audit-log.jsonl` / `AuditLog::latest_for(docx_path)` 读最近一条
  4. atomic write：先写 temp file 再 rename
- **验收标准**:
  - manifest 写读 round-trip 测试通过
  - `verify_against_disk` 能识别 mtime 不一致
  - 并发 append 不丢条目（用 file lock）

#### Task L2.3: thesis-mcp-server 骨架 + health check
- **文件范围**: `crates/thesis-mcp-server/src/main.rs` / `health.rs` / `tools/mod.rs` + `init.rs` + `audit.rs`（不实现 write_section/revise）
- **依赖**: L1.2
- **实施步骤**:
  1. **main.rs**: rmcp stdio server，注册 4 个 tool 名（init / write_section / revise / audit），先实现 init + audit 两个
  2. **health.rs**: server 启动时校验 audit.py wrapper 健康 / ooxmlsdk 版本 / .thesis/ 目录权限
  3. **tools/init.rs**: 创建 `.thesis/{progress.md,outline.md,format-spec.md}` 模板 + 校验 docs/ 存在
  4. **tools/audit.rs**: 调用 thesis-audit::audit_full，返回 AuditResult JSON
  5. tracing 写日志到 stderr（CC 会捕获）
- **验收标准**:
  - `cargo run --bin thesis-mcp-server` 能启动 stdio server
  - 用 `printf '{"jsonrpc":"2.0","method":"tools/list",...}' | thesis-mcp-server` 能列 4 个工具
  - audit tool 能跑通 valid_simple.docx 返回 PASS

### Layer 3（写入工具 + hook 收口，依赖 L2）

#### Task L3.1: thesis-mcp-server 写入工具实现
- **文件范围**: `crates/thesis-mcp-server/src/tools/write_section.rs` / `revise.rs`
- **依赖**: L2.1 + L2.2 + L2.3
- **实施步骤**:
  1. **write_section.rs**:
     - 输入：docx_path / section_spec (JSON：title + heading_level + paragraphs[] + figures[] + references[]) / style_spec
     - 流程：① ooxmlsdk 加载 docx →  ② 按 section_spec 写新章节（typed model 改 body / 加 paragraphs / 加 references）→ ③ 写到 temp docx → ④ thesis-audit::audit_full(temp) → ⑤ FAIL 则删 temp 抛错；PASS 则 manifest + atomic rename
     - 关键：ooxmlsdk root_element_mut 改 typed 部分；quick-xml 处理 ooxmlsdk 未暴露的 raw XML（如 numbering.xml lvlText 真验证）
  2. **revise.rs**:
     - 输入：docx_path / edits (Vec<EditOp>) / color (默认 blue RGB 0,0,255)
     - 流程：① 备份到 docs/.backups/ → ② ooxmlsdk 加载 → ③ 对每个 edit op 找目标段落/run，加蓝色 run / 不留 strike → ④ 写 temp → ⑤ audit 含 F.5.2 修订项 → ⑥ FAIL/PASS 处理同上
  3. EditOp 类型：Insert / Delete / Replace / FormatChange，每种独立处理函数
- **验收标准**:
  - write_section: 给定 spec 写完后 audit 返回 PASS；故意构造违规 spec audit 返回 FAIL + manifest 不生成
  - revise: 不留 strike 残留 / 字体继承自原段 / 蓝色 RGB 正确
  - 中途失败（如磁盘满）docx 原文件不损坏

#### Task L3.2: thesis-hook binary（PreToolUse / Stop）
- **文件范围**: `crates/thesis-hook/`
- **依赖**: L2.2（manifest 读写）
- **实施步骤**:
  1. **main.rs**: clap 子命令 `pre-tool-use` / `post-tool-use` / `stop`
  2. **pre_tool_use.rs**:
     - stdin 读 CC 传入的 hook JSON
     - 解析 tool_name + tool_input
     - 拦截规则（HC-26）：
       - tool_name in [Write/Edit/MultiEdit/NotebookEdit] + file_path 匹配 `*.docx` → exit 2 + 提示用 mcp__thesis__write_section
       - tool_name == Bash + command 含 `python.*docx` / `node.*docx` / `zip.*docx` / `cp.*\.docx` / `mv.*\.docx` / `libreoffice.*--convert` / `osascript.*Word` / `printf.*>.*docx` 且不是白名单（`thesis_docx_audit.py` 本身）→ exit 2
       - tool_name == Agent + prompt 含 "thesis" / "论文" / "docx" 关键词 → exit 2 + 提示 subagent 不继承 skill
     - 其他 → exit 0
  3. **stop.rs**:
     - 读 transcript（CC 传 transcript_path）
     - 识别本会话是否 /thesis 域（含别名/间接调用，SC-2）
     - 扫描 `.thesis/audit-log.jsonl` 拿本会话所有 manifest
     - 对每个 manifest verify_against_disk → TOCTOU 违规 → exit 2
     - 扫描 docs/*.docx mtime > 会话开始 但 manifest 没记录 = subagent 偷写 → exit 2
     - catch 分类：thesis 域内异常 → exit 2 / 域外 → exit 0（HC-4 + SC-5）
  4. **post_tool_use.rs**（可选，主要靠 MCP write 内置 audit）：
     - 如果用户/Claude 走非 MCP 路径写了 docx（不应该发生但兜底）→ 触发外部审计 + 写 manifest
- **验收标准**:
  - 10 类对抗测试 fixture（tests/adversarial/）拦截率 ≥ 9/10
  - 非 thesis 任务 Stop hook 始终 exit 0
  - PreToolUse 误判率 0（白名单 audit.py / 普通代码 Write 不被阻断）

### Layer 4（审计能力深化 + 防御层自保护，依赖 L3）

#### Task L4.1: WordprocessingML 全包扩展（HC-31）
- **文件范围**: `crates/thesis-audit/src/{headers_footers,comments,footnotes,textbox,tracked_changes,styles}.rs`（实际在 L2.1 已建文件，本 task 是把"骨架"做成"完整覆盖"）
- **依赖**: L3.1（已有写入工具能生成各类边界 fixture）
- **实施步骤**:
  1. 每个 part 类型独立扫描函数 + 独立测试
  2. ooxmlsdk 协同（codex 给的规则）：
     - 进入 typed 模式后只用 root_element_mut，不再 set_data
     - 进入 raw 模式前先 unload_root_element
     - untouched parts 保持 raw bytes
  3. unknown elements byte-for-byte passthrough（依赖 ooxmlsdk untouched parts 默认行为 + write_section 时只触碰目标 part）
- **验收标准**:
  - fixture violation_footer_blackword.docx 能识别页脚黑词
  - fixture violation_textbox_blackword.docx 能识别文本框黑词
  - 修改任意 part 后未触碰 part 字节级一致（用 sha256 比对）

#### Task L4.2: 防御层自保护 + 对抗测试套件
- **文件范围**: `crates/thesis-hook/src/pre_tool_use.rs`（加 self-protect）/ `tests/adversarial/` 全部 10 个 fixture
- **依赖**: L3.2
- **实施步骤**:
  1. pre_tool_use.rs 加：阻断对 `~/.claude/hooks/thesis*` / `~/.local/share/claude/.../thesis*` / `~/CodeCoding/Code/自研项目/Skills/thesis-mcp/{src,target}` 的写入（HC-22）
  2. 写 10 个对抗测试 case（每个 case 含完整 stdin JSON 模拟 CC hook event + 预期 exit code）
  3. CI 脚本：`scripts/ci-test.sh` 跑 `cargo test --workspace` + 10 类对抗 case
- **验收标准**:
  - 10 类对抗 case 拦截率 ≥ 9/10（OK-8）
  - 防御层文件被 Claude Write 工具拦截 → exit 2

### Layer 5（SKILL.md 重构 + 集成验证）

#### Task L5.1: SKILL.md 重构
- **文件范围**: `~/.claude/skills/thesis/SKILL.md`（不在本项目目录，但作为最终交付一部分）
- **依赖**: L4 全部完成
- **实施步骤**:
  1. 现有 22.1K → 收敛到 ~8K
  2. 删除 F.0 入口三连、F.4 终端输出政策、F.5 自检表必输等 HARD-GATE 文字约束（已由 hook 强制）
  3. 保留：Phase 路由（F.1）/ 文件位置契约（F.2）/ Phase 0-3 工作流（F.6-F.9）/ 与 MCP 工具的协议（新加）
  4. 加：`/thesis` 触发必先调 mcp__thesis__init 校验环境
  5. 加：所有写 docx 走 mcp__thesis__write_section 或 revise（不允许 Bash 直写）
- **验收标准**: SKILL.md ≤ 8K + 所有 HARD-GATE 都能映射到 hook 或 MCP 工具

#### Task L5.2: 安装脚本 + 集成测试
- **文件范围**: `scripts/install.sh` / `scripts/ci-test.sh`
- **依赖**: L4 全部完成
- **实施步骤**:
  1. install.sh：cargo build --release → 把 binary 软链到 `~/.claude/hooks/thesis-*` → 改 settings.json 注册 hook + 注册 mcpServers
  2. 写 README 含安装/卸载/调试步骤
  3. ci-test.sh：cargo fmt --check + cargo clippy + cargo test --workspace + adversarial test
- **验收标准**: install.sh 一键安装到 CC，重启 CC 后 `/thesis` 触发能调 MCP 工具

---

## 文件冲突检查

| Task | 文件范围 | 与其他 Task 冲突 |
|---|---|---|
| L1.1 | Cargo.toml (root) / .gitignore | 无 |
| L1.2 | crates/thesis-types/ | 无 |
| L2.1 | crates/thesis-audit/ | 无 |
| L2.2 | crates/thesis-manifest/ | 无 |
| L2.3 | crates/thesis-mcp-server/{main.rs,health.rs,tools/{mod,init,audit}.rs} | 与 L3.1 同 crate 但不同文件 → 无冲突 |
| L3.1 | crates/thesis-mcp-server/tools/{write_section,revise}.rs | 与 L2.3 同 crate 但不同文件 |
| L3.2 | crates/thesis-hook/ | 无 |
| L4.1 | crates/thesis-audit/{各 part}.rs | **与 L2.1 同文件** → 必须 L2.1 完成后做 |
| L4.2 | crates/thesis-hook/src/pre_tool_use.rs + tests/adversarial/ | 与 L3.2 同文件 → L4.2 接力 L3.2 |
| L5.1 | ~/.claude/skills/thesis/SKILL.md | 在用户目录外，独立 |
| L5.2 | scripts/ + README.md | 无 |

✅ 同 Layer 内 Task 无冲突；跨 Layer 必依赖前层完成。

---

## 并行分组

| Layer | 可并行 Task | 工程量 | 累计 |
|---|---|---|---|
| L1 | L1.1 → L1.2（串行，L1.2 依赖 L1.1） | 0.5 + 1 = 1.5d | 1.5d |
| L2 | L2.1 ∥ L2.2 ∥ L2.3（3 任务并行） | max(4d, 1.5d, 1.5d) = 4d | 5.5d |
| L3 | L3.1 ∥ L3.2（2 任务并行） | max(2.5d, 2d) = 2.5d | 8d |
| L4 | L4.1 ∥ L4.2（2 任务并行） | max(2.5d, 2d) = 2.5d | 10.5d |
| L5 | L5.1 ∥ L5.2（2 任务并行） | max(1d, 1.5d) = 1.5d | 12d |
| **缓冲** | 跨 Layer 集成调试 + bug 修 | +2-9d | **14-21d** |

单人顺序执行约 18-20 人日；并行（多 builder / spawn agent）可压到 14-15 人日。

---

## 关键风险点（来自 research 文件 + codex 实证）

| 风险 | 概率 | 影响 | 缓解 |
|---|---|---|---|
| ooxmlsdk 0.6.x typed model 对某些 WordprocessingML 元素未完全暴露（未知元素 DOM 编辑是 future work） | 中 | 部分扩展需 fallback 到 quick-xml raw | L2.1 预留 fallback 接口，遇到不支持的元素切 quick-xml |
| MCP server 崩溃导致 Claude 失去 mcp__thesis__* 工具（RISK-3） | 低 | Claude 瘫痪 | L2.3 health check + 用户重启提示 + PreToolUse 降级为警告 |
| Rust libxml2 系统依赖冲突（如果未来引入 libxml dev feature） | 低 | macOS 用户需 brew install libxml2 | L1.1 在 Cargo.toml 标 dev-dependency + scripts/install.sh 检测 |
| cargo build 首次编译时间长（~5-10 分钟） | 高 | 用户首次安装等待 | install.sh 预编译二进制选项（GitHub release） |
| MCP_TIMEOUT 60s vs ooxmlsdk 大 docx 解析慢 | 中 | 超时 fail | L2.1 大 docx 分段处理 + 流式 audit |
| ooxmlsdk maintainer 活跃度（最新 stable 0.6.1） | 低 | 长期 fork 风险 | 不依赖 unstable feature + 关键 issue 自己 PR |
| Claude 学会用 mcp__thesis__init 跳过 audit | 低 | 防御漏洞 | L3.1 write_section 内置 audit 不依赖 init 调用次序 |

---

## 不变约束（来自 research 文件，本 plan 不可违反）

- HC-1/HC-2/HC-4：现有 thesis-stop-guard.js 漏洞 → L3.2 thesis-hook binary 完全替代
- HC-5：bypassPermissions 下 Bash 拦截只能在 hook 脚本解析命令字符串 → L3.2 pre_tool_use.rs 实现
- HC-11：subagent 委派绕过 → L3.2 Agent 工具 prompt 含 thesis 关键词阻断
- HC-13：printf+zip 字节构造 → L3.2 Bash 命令含 `zip.*docx` 或 `printf.*>.*docx` 阻断
- HC-17：codeagent-wrapper 进程不出现在 transcript → L3.2 Stop hook 无差别扫 mtime
- HC-22：bypassPermissions 下防御层文件可改 → L4.2 self-protect 阻断
- HC-23：TOCTOU → L3.2 Stop hook + manifest verify_against_disk
- HC-25：fail-closed → L3.2 audit 失败/超时/manifest 缺失一律 exit 2
- HC-26：Bash docx 写入阻断规则 → L3.2 pre_tool_use.rs 实现
- HC-27/HC-28：MCP server 健康检查 + fallback → L2.3 health.rs
- HC-29：无差别 mtime 扫 → L3.2 Stop hook
- HC-30：manifest 锁定本轮目标 → L2.2 + L3.2
- HC-31：完整 WordprocessingML 包审计 → L4.1
- HC-32：自检表 hook 注入 → L3.2 + L4.2

---

## 下一步动作

1. 用户在本目录（`/Users/oi/CodeCoding/Code/自研项目/Skills/thesis-mcp/`）重开会话
2. 新会话首次任务：执行 L1.1 + L1.2（cargo workspace 初始化 + thesis-types crate）
3. L1 完成后进入 L2 三任务并行（建议 spawn 3 个 builder subagent，每个负责一个 crate）
4. 每个 Layer 完成必跑 `scripts/ci-test.sh` 验证不破坏既有 layer

---

## 元信息

- 来源：3 轮 codex 实证（语言决策 / 库选型 / ooxmlsdk 协同模式）+ 研究文件 32 条硬约束
- 总工程量：14-21 人日（×1.5 风险缓冲）
- 技术栈：Rust + ooxmlsdk 0.6.1 + quick-xml 0.40.1 + xot + xee-xpath
- 实施 owner：用户在本目录重开会话推进
- PLAN 版本：v1（2026-05-17）
