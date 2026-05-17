# Team Research: thesis-skill-enforcement

## 增强后的需求

**目标**：让 Claude 在执行 `/thesis` skill 时无法偷懒（伪 PASS / 跳过自检 / 静默通过 / 绕过审计），通过工具级强制校验取代当前依赖 SKILL.md 文字 HARD-GATE + Stop hook 软兜底的现状。

**范围**：
- `~/.claude/skills/thesis/SKILL.md` (22.1K) + `references/`
- `~/.claude/hooks/thesis-stop-guard.js` (30.3K) + `thesis_docx_audit.py` (20.7K)
- `~/.claude/settings.json` hooks 节
- 可选新增：`~/.claude/scripts/thesis-*` wrapper / `mcp-server-thesis/`

**候选方案**：
- A. MCP 工具替代法（thesis MCP server + PreToolUse 强制走 MCP）— **用户最终选择**（Q8 2026-05-17）
- B. 全栈防御 hook（PreToolUse 拦 + PostToolUse 注入 + Stop 自跑脚本 + 自检表完整性强制）— **黑名单本质，原理性弱于 A**
- C. 混合（B 为主 + A 作为长期演进的 opt-in）— 我前期错误推荐，理由是把 MCP 价值误判为"分发场景"，用户当场纠正：MCP 核心价值是白名单收口写入入口，与是否分发无关

**验收**：通过 10 种对抗性测试（每种 5 次），偷懒拦截率 ≥ 90%。对抗测试覆盖：伪 PASS / 跳行 / Bash 字节构造 / subagent 委派 / `__import__('doc'+'x')` AST obfuscation / 短回复阈值绕过 / 后台延迟写 / 隐藏区域违规 / 空 numPr 伪结构 / 修订 strike 残留。

**硬约束**：本机自用、不分发、不打包 MCP 给第三方。

---

## 约束集

### 硬约束（HC）— 设计必须满足

**现状机制漏洞类**（不修这些，任何方案都站不住）

- [HC-1] Stop hook 配置 timeout 5s，但内部 spawnSync audit.py timeout 15s——**大 docx 或慢磁盘下外层 hook 必被 CC 先杀**，门禁形同虚设 — 来源：codex
- [HC-2] thesis-stop-guard.js 遇到 /thesis 后任意非工具用户文本就 exit(0) 跳过——**用户中途补充要求或纠错 = 当轮审计被关闭** — 来源：codex
- [HC-3] Stop hook 只向后扫 30 条用户消息——长会话 + 自动压缩 + 密集工具结果会让 /thesis 触发点滚出扫描窗口 — 来源：codex
- [HC-4] catch 内 process.exit(0) 静默通过——任何异常 = 假装 PASS — 来源：我（前一轮 review #6 的根因）
- [HC-5] CC defaultMode=bypassPermissions 下 Bash 不靠权限阻断，所有 docx 写入识别**只能在 hook 脚本里解析命令字符串** — 来源：codex

**审计能力盲区类**（audit.py 当前根本看不到的违规位置）

- [HC-6] audit.py 只解析 word/document.xml——**页眉/页脚/脚注/尾注/批注/文本框/SmartArt/嵌入对象/tracked changes/styles.xml/numbering.xml/relationships 全部不在审计闭环** — 来源：codex
- [HC-7] body.findall(w:p) 只覆盖 body 直接子段落——**表格内正文段落 / 文本框内段落不进多数正文规则扫描** — 来源：codex
- [HC-8] numPr 检查只验段落存在 numPr，不验 numbering.xml 中 numId/abstractNum/lvlText 是否真匹配 E.5.7/E.5.8 章节编号或参考文献列表——**空 numPr 可伪通过** — 来源：codex
- [HC-9] G.1/G.19 中文 200 字阈值——**短回复可完成实质 docx 修改但跳过自检表和 progress 标记要求** — 来源：codex
- [HC-10] PROGRESS-UPDATED 只是响应字符串标记，未验证 progress.md 实际内容/mtime/章节状态/引用清单 — 来源：codex

**绕过路径类**（Claude 偷懒的所有已知通道）

- [HC-11] subagent 委派绕过：Claude 用 Agent 工具 spawn general-purpose subagent 写 docx，子 agent 不继承 skill、不触发父会话 hook、不读 HARD-GATE — 来源：gemini（致命，前一轮 14 项全未覆盖）
- [HC-12] AST obfuscation：`__import__('doc'+'x')` 拼接 import / `getattr(d,'add_'+'paragraph')` 反射调用绕 regex 关键词拦截 — 来源：gemini
- [HC-13] printf + zip 字节级构造 docx XML 绕过任何 python-docx 拦截 — 来源：我 #3
- [HC-14] PostToolUse on Write `*.docx` 永远不触发（Claude 100% 走 Bash 写 docx）—— 全栈方案 #4 的设计前提错误 — 来源：我 #4
- [HC-15] Read+目测断言绕过：Claude 声称"已读 docx XML，目测无违规" — 来源：我 #14
- [HC-16] Baseline 倒退：Claude 看到 FAIL 把违规段删了重生成绕过历史对比 — 来源：我 #12
- [HC-17] codeagent-wrapper 启动的进程不出现在主会话 transcript 的可解析工具调用中 — 来源：codex

**写入路径类**（MCP 设计约束）

- [HC-18] MCP 协议只能提供工具入口，**不能天然禁止 Bash/Write/其他 MCP**——MCP 方案 A 若无 PreToolUse 配合，"只能用 MCP 写 docx" 不成立 — 来源：codex
- [HC-19] MCP_TIMEOUT 默认 60000ms，stdio JSON 不适合传输大体积 docx 二进制——**MCP 工具必须传路径 + 结构化操作，不能传文件内容** — 来源：codex
- [HC-20] MCP write 必须事务式：temp docx + audit + manifest + atomic rename，否则中途异常 = 损坏文件 — 来源：codex
- [HC-21] additionalContext 注入有 ~10K token 上限——完整 audit 报告必超限，必须注入压缩版 + 写文件让 Claude Read — 来源：我 #5

**门禁自篡改类**（防御层自己也是攻击目标）

- [HC-22] bypassPermissions 模式下模型可改 ~/.claude/hooks/、settings.json、MCP server 源码——**必须额外阻断对防御层文件的写入** — 来源：codex
- [HC-23] TOCTOU：Stop hook 审计通过后，后台进程/异步 shell/外部 Word/延迟写入仍可改 docx——审计后的文件不可信 — 来源：codex
- [HC-24] PreCompact 反向陷阱：审计反馈太啰嗦 → 触发 PreCompact → 把违规规则本身压缩掉 — 来源：gemini（加深我 #13）

### 软约束（SC）— 设计应当满足

- [SC-1] Claude 短回复（< 200 中文字）也必须触发自检——不能用阈值放行 — 来源：codex
- [SC-2] /thesis 识别不能只靠 transcript 显式 `<command-name>/thesis</command-name>` 字符串——别名、间接调用、复制 skill 内容执行都应进入门禁域 — 来源：codex
- [SC-3] cwd fallback 只扫 cwd/docs/*.docx 或 cwd/*.docx 第一个文件——多论文/多版本/软链场景必须显式 manifest 而非猜测 — 来源：codex
- [SC-4] 自检表里 PASS 行的命中数必须能映射到同轮 manifest/audit JSON——Claude 写"脚本输出: 0 处"必须被 hook 重算对比 — 来源：我 #8 + codex
- [SC-5] Stop hook 不能误阻断非 thesis 任务——catch 内分类（thesis 域内异常 → exit(2) / 域外或判断不出 → exit(0)） — 来源：我 #6 + codex
- [SC-6] DX 退化警戒：hook 噪音过多会让 Claude 进入"I apologize"循环—— audit 反馈必须可操作（指明段号 + 修复方向）而非纯告状 — 来源：gemini

### 依赖关系（DEP）

- [DEP-1] CC hook 生命周期 + JSON 输入 schema + exit code 语义 + additionalContext 注入语义——任何 hook 改动都要对齐 — 来源：codex
- [DEP-2] WordprocessingML 包结构：document.xml / numbering.xml / styles.xml / rels / comments / footnotes / endnotes / headers / footers——审计完整性依赖 — 来源：codex
- [DEP-3] 本机其他全局 hooks（fp-stop-guard.js / clawd-hook.js / ensoai-hook.cjs / session-gate-check / rtk-rewrite.sh）——thesis hook 改造不能干扰其他 hook — 来源：codex
- [DEP-4] .thesis/{progress.md,outline.md,format-spec.md} + docs/.backups 路径契约——manifest 设计要锚定这些 — 来源：codex + SKILL.md F.2

### 风险（RISK）

- [RISK-1] 工程量普遍偏乐观：MCP 实际 7-10 人日（不是 5-6）、全栈实际 3-5 人日（不是 2-3），test fixture 总被低估 — 缓解：把估算 ×1.5 + 留 buffer
- [RISK-2] PreToolUse 关键词拦截误伤 audit 自身（`Document(` / `add_paragraph` 是 audit 脚本字面字符串）— 缓解：白名单 `~/.claude/scripts/thesis-*` + `~/.claude/hooks/thesis*` 路径前缀
- [RISK-3] MCP server 单点故障无 fallback，崩 = Claude 失去工具瘫痪 — 缓解：定义 fallback（PreToolUse 降级为警告 + 提示用户重启）
- [RISK-4] Stop hook 无法区分"任务完成" vs "用户中止 Esc"——误对中止场景跑 audit 干扰 — 来源：gemini — 缓解：识别用户最后消息是否含中止信号
- [RISK-5] MCP stateful（内存持文档）vs stateless（每次读盘）决策未定——影响并发、崩溃恢复、性能 — 缓解：先做 stateless 简单版本
- [RISK-6] 部分写入风险：docx 正在保存或 zip 未完整落盘时 audit.py 解析失败，fail-open 静默放行 / fail-closed 误阻断 — 缓解：检测 ~$ 临时锁文件 + 重试机制

---

## 成功判据（OK）

- [OK-1] /thesis 上下文中直接 Write/Edit/MultiEdit/NotebookEdit 写 `*.docx` 被 PreToolUse 明确阻断
- [OK-2] Bash 中 python/node/ruby/perl/osascript/zip/libreoffice/cp/mv 对目标 docx 的创建或修改路径能被阻断或要求走 MCP/wrapper
- [OK-3] 每次写入生成审计 manifest：docx 绝对路径 + sha256 + mtime + 操作类型 + 规则命中数 + audit 脚本版本 + nonce
- [OK-4] Stop hook 不再 cwd 猜目标，而是读最近一次 MCP/wrapper manifest 或显式 project state
- [OK-5] audit.py 超时/解析失败/路径不唯一/manifest 缺失时在 /thesis 场景 fail-closed 并输出可操作错误
- [OK-6] 自检表 PASS 能映射到同轮 manifest/audit JSON 的规则 ID、命中数、docx hash——hook 自跑脚本对比，不信 Claude 写的数
- [OK-7] progress.md 实际 mtime/hash/章节状态被验证，而不是只查 PROGRESS-UPDATED 字符串
- [OK-8] 10 类对抗用例可复现并被拦截：后台延迟写 / 手工 zip 构造 / 空 numPr / 隐藏页脚违规 / 伪造脚本输出 / 修改 hook 自身 / subagent 委派 / AST obfuscation / 短回复阈值绕过 / 修订 strike 残留
- [OK-9] 非 /thesis 普通任务不受 thesis 门禁影响，Stop hook 对非 thesis transcript 稳定 exit(0)
- [OK-10] 任何失败都不破坏原 docx：原文件保留、临时文件清理、docs/.backups 有可恢复版本
- [OK-11] 审计覆盖完整 WordprocessingML 包（document.xml + numbering.xml + styles.xml + headers/footers + footnotes/endnotes + comments + textboxes）
- [OK-12] hook 反馈可操作：FAIL 输出含段号 + 规则 ID + 修复方向，而非纯告状

---

## 开放问题（已解决，2026-05-17 用户确认）

| ID | 问题 | 用户答案 |
|---|---|---|
| Q1 | /thesis 是否接受 fail-closed？ | **接受** — audit 解析失败/超时/路径不唯一一律阻断 |
| Q2 | 是否完全禁止 Bash 对 docs/*.docx / .thesis/*.md / ~/.claude/hooks/thesis* 的写入？ | **是** |
| Q3 | MCP server 定位 | **推荐但 Bash 白名单仍可** |
| Q4 | 是否覆盖 subagent / codeagent-wrapper / 外部编辑器 / 人工 Word 修改后的 docx 审计？ | **是** |
| Q5 | 论文项目路径策略 | **支持多版本，靠 manifest 锁定本轮目标** |
| Q6 | 审计范围是否覆盖完整 WordprocessingML 包？ | **是** |
| Q7 | 自检表强制策略 | **机器可校验 + 完整性强制** |
| Q8 | 工程量预算 | **直接从 MCP 开始项目**（跳过分阶段） |

### Q2 ⊗ Q3 解读（待用户反向纠正）

Q2 "禁 Bash 写 docx" 与 Q3 "Bash 白名单仍可" 表面冲突。我的解读：
- **禁止**：Bash 中任何 python/node/ruby/perl/osascript/zip/libreoffice/cp/mv 直接对 `docs/*.docx`、`.thesis/*.md`、`~/.claude/hooks/thesis*` 的写入或修改命令
- **白名单只放行 audit 脚本本身**：`~/.claude/hooks/thesis_docx_audit.py` 的 Bash 调用允许（Claude 主动跑诊断 audit 场景）
- **MCP server 不走 Bash**：由 CC 通过 settings.json `mcpServers` 自动启动，不需要 Bash 白名单
- **核心写入唯一入口**：MCP 工具 `mcp__thesis__write_section` / `revise` / `audit` / `init`
- **不引入 wrapper 脚本**：Q8 选直接 MCP，跳过 `~/.claude/scripts/thesis-*` wrapper 中间层

用户 2026-05-17 补充 Q2 精确化为"是 + 白名单 audit 脚本本身"，本段已据此收窄。

### 衍生硬约束（由用户答案产生）

- [HC-25] /thesis 场景 fail-closed 是 P0 行为：audit 解析失败/超时/manifest 缺失/docx 路径不唯一/MCP server 不可用 → 一律 exit 2 阻断回复 — 来源：Q1
- [HC-26] PreToolUse 必须阻断 Bash 命令含 `python.*docx`、`node.*docx`、`zip.*docx`、`cp.*\.docx`、`mv.*\.docx`、`libreoffice.*--convert`、`osascript.*Word`，且目标路径在 `docs/`、`.thesis/`、`~/.claude/hooks/thesis*` 下；**白名单只放行 `~/.claude/hooks/thesis_docx_audit.py` 本身的调用**（Claude 主动跑诊断 audit 用），MCP server 由 CC settings.json mcpServers 自启不需 Bash 白名单 — 来源：Q2 精确版（用户 2026-05-17 补充）+ Q3
- [HC-27] MCP server 必须作为 P0 实施（不是 P4）— Q8 直接 MCP，否则 fail-closed (HC-25) 会让用户第一天就被锁死 — 来源：Q8 + RISK-3 + HC-25 联合推导
- [HC-28] MCP server 必须有健康检查 endpoint + 启动失败 fallback（PreToolUse 降级为警告 + 提示用户重启 + 临时打开 wrapper 路径）— 来源：HC-27 + RISK-3
- [HC-29] Stop hook 必须无差别扫描会话期间所有 docx mtime 更新，不论写入来源（主会话 Bash / subagent / codeagent-wrapper / 外部 Word / 手工修改）— 来源：Q4
- [HC-30] manifest 是本轮目标唯一锁定方式，不再用 cwd 猜路径；manifest 不存在 = 本轮无 thesis 写入 = Stop hook 不审计；manifest 存在但 docx 实际 mtime > manifest 记录 = TOCTOU 违规 → 阻断 — 来源：Q5 + HC-23
- [HC-31] audit.py 必须扩展到完整 WordprocessingML 包：document.xml + numbering.xml + styles.xml + headers/footers + footnotes/endnotes + comments + textboxes + SmartArt + tracked changes + relationships，每个组件独立可测 — 来源：Q6
- [HC-32] 自检表是 hook 直接生成并强制注入到 assistant 响应（PostToolUse `additionalContext` 压缩版 + `.thesis/audit-log.jsonl` 完整版），Claude 必须原样引用 manifest 中的规则 ID + 命中数 + docx hash，hook 用 hash 比对识别篡改 — 来源：Q7 + HC-32 + OK-6

---

## 推荐路径（用户已选 Q8 = 直接 MCP）

总工程量：**10-15 人日**（基础估算 7-10 ×1.5 风险缓冲）

| 阶段 | 工程量 | 内容 | 覆盖约束 |
|---|---|---|---|
| **P0 紧急修补 + MCP server 骨架** | 1.5 天 | (a) HC-1 timeout 对齐 / HC-2 用户文本不 exit / HC-4 catch 不静默——必须先做，否则后续依赖现有 hook 的部分仍漏；(b) MCP server stdio 骨架 + health check + init/audit 两个最简工具 | HC-1/2/4 + HC-27/28 |
| **P1 MCP 写入工具 + manifest 协议** | 3 天 | write_section + revise 工具实现，事务式写入（temp + audit + manifest + atomic rename），manifest 协议落地（docx 绝对路径 + sha256 + mtime + 操作类型 + 规则命中数 + audit 版本 + nonce） | HC-19/20 + HC-30 + OK-3/4/10 |
| **P2 PreToolUse 拦截层** | 1.5 天 | (a) Bash 命令解析 + 白名单 + 阻断规则（HC-26）；(b) Write/Edit/MultiEdit/NotebookEdit on *.docx 阻断；(c) Agent 工具委派含 docx 关键词阻断（HC-11） | HC-11/14/15/26 + OK-1/2 |
| **P3 audit.py WordprocessingML 全包扩展** | 2-3 天 | 表格内段落 / 页眉页脚 / 脚注尾注 / 批注 / 文本框 / numbering.xml 真验证（验 numId/abstractNum/lvlText）/ styles.xml / tracked changes / SmartArt | HC-6/7/8 + OK-11 |
| **P4 Stop hook 改造** | 1.5 天 | (a) 异常分类：thesis 域内→exit 2 / 域外→exit 0（HC-4 + SC-5）；(b) 无差别 mtime 扫（HC-29）；(c) 读 manifest 不猜 cwd（HC-30）；(d) 自检表完整性 + hash 比对识别 Claude 伪造（HC-32 + SC-4） | HC-3/4/10/22/23/29/30/32 |
| **P5 防御层自保护 + 对抗性测试** | 2 天 | (a) PreToolUse 阻断对 ~/.claude/hooks/thesis*、MCP server 源码、settings.json 的写入（HC-22）；(b) 10 类对抗用例 fixture（OK-8）+ CI 跑测 | HC-22 + OK-8 |
| **P6 SKILL.md 重构** | 1 天 | 22.1K 收敛到 ~8K，HARD-GATE 文字约束改为指向 MCP 工具调用 + 已知 hook 验证点；删除模型自律部分 | SC-6 + 维护性 |

**为什么 MCP 是核心方案，不是 over-engineering**：全栈防御 hook 本质是**黑名单**（列 Bash 禁止模式 + regex 拦截），AST obfuscation / 字节构造 / subagent 委派 / 字符串拼接 import 等绕过路径**穷举不完**。MCP 工具入口 + PreToolUse 收口是**白名单**（只有 MCP 工具是合法写入入口，其他全禁），白名单在攻防对抗中**原理性优于**黑名单。任务的核心诉求是"强制让 Claude 无法绕过"，这是控制问题不是工程量问题——本机自用同样适用。

---

## 元信息

- 来源：用户 14 项 review + gemini 3.1 pro（DX/对抗）+ codex（系统/协议/审计）
- 总约束数：硬约束 32（24 + 用户答案衍生 8）/ 软约束 6 / 依赖 4 / 风险 6 / 成功判据 12 / 开放问题 0（已全解决）
- 文件路径：`/Users/oi/.claude/team-plan/thesis-skill-enforcement-research.md`
- 下一步：用户已答 Q1-Q8（2026-05-17）→ 运行 `/clear` → 执行 `/ccg:team-plan thesis-skill-enforcement` 进入规划阶段，按 P0-P6 分阶段产出零决策实施计划

Q1:接受;Q2:是;Q3:推荐但 Bash 白名单仍可;Q4:是;Q5:支持多版本，靠 manifest 锁定本轮目标;Q6:是;Q7:机器可校验 + 完整性强制;Q8:直接从 MCP 开始项目;
