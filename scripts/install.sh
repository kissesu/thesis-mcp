#!/usr/bin/env bash
# scripts/install.sh — thesis-mcp 开发者本地安装脚本（dev fallback）
#
# 注意：自 v0.1.0 起本项目主路径是 Claude Code plugin：
#       claude plugin install kissesu/thesis-mcp
# 本脚本仅保留作开发者本地构建（改 Rust 源码后想本地测试 / 仓库未发布 release 时）。
#
# 用法:
#   bash scripts/install.sh             # 正常安装（本地构建）
#   bash scripts/install.sh --dry-run   # 打印计划，不执行
#   bash scripts/install.sh --uninstall # 卸载
#   bash scripts/install.sh --uninstall --restore-backup  # 卸载并还原最近备份
#
# 抑制 deprecation warning：export THESIS_MCP_SUPPRESS_DEPRECATION=1
#
# @author Atlas.oi
# @date   2026-05-18

set -euo pipefail

# ============================================================
# Deprecation warning（dev fallback 提示）
# ============================================================
if [ "${THESIS_MCP_SUPPRESS_DEPRECATION:-0}" != "1" ]; then
    cat >&2 <<'EOF'
┌─────────────────────────────────────────────────────────────┐
│ [thesis-mcp] 提示：你正在使用开发者本地构建路径              │
│                                                              │
│ 普通用户应改用 Claude Code plugin（一行装完，自动拉预编译二进制）：│
│   claude plugin install kissesu/thesis-mcp                  │
│                                                              │
│ 仅在改 Rust 源码 / 仓库未发布 release / 网络受限时用本脚本。 │
│ 抑制此提示：export THESIS_MCP_SUPPRESS_DEPRECATION=1         │
└─────────────────────────────────────────────────────────────┘
EOF
    sleep 2
fi

# ─────────────────────────────────────────────────────────────────────────────
# 常量
# ─────────────────────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

HOOKS_DIR="${HOME}/.claude/hooks"
SKILLS_DIR="${HOME}/.claude/skills"
SETTINGS_JSON="${HOME}/.claude/settings.json"
CLAUDE_JSON="${HOME}/.claude.json"

HOOK_LINK_MCP="${HOOKS_DIR}/thesis-mcp-server"
HOOK_LINK_HOOK="${HOOKS_DIR}/thesis-hook"

MCP_ENTRY_KEY="thesis"

# ─────────────────────────────────────────────────────────────────────────────
# 安装模式自检（v0.1.3 起支持 tarball 模式）
#
# tarball 模式：install.sh 在 tarball 解压目录，binary 在 ${SCRIPT_DIR}/bin/
#   适用：用户下载 release tarball 后解压并运行本脚本
#
# dev 模式：install.sh 在项目 scripts/ 子目录，binary 在 ${PROJECT_DIR}/target/release/
#   适用：开发者本地 cargo build 后运行本脚本
# ─────────────────────────────────────────────────────────────────────────────
if [[ -x "${SCRIPT_DIR}/bin/thesis-hook" ]] && [[ -x "${SCRIPT_DIR}/bin/thesis-mcp-server" ]]; then
    INSTALL_MODE="tarball"
    PROJECT_DIR="$SCRIPT_DIR"
    MCP_SERVER_BIN="${SCRIPT_DIR}/bin/thesis-mcp-server"
    HOOK_BIN="${SCRIPT_DIR}/bin/thesis-hook"
    SKILL_SRC="${SCRIPT_DIR}/skills/thesis"
elif [[ -f "${SCRIPT_DIR}/../Cargo.toml" ]]; then
    INSTALL_MODE="dev"
    PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
    MCP_SERVER_BIN="${PROJECT_DIR}/target/release/thesis-mcp-server"
    HOOK_BIN="${PROJECT_DIR}/target/release/thesis-hook"
    SKILL_SRC="${PROJECT_DIR}/skills/thesis"
else
    echo "[ERROR] 无法识别安装模式：" >&2
    echo "  既无 ${SCRIPT_DIR}/bin/{thesis-hook,thesis-mcp-server} (tarball 模式)" >&2
    echo "  也无 ${SCRIPT_DIR}/../Cargo.toml (dev 模式)" >&2
    exit 1
fi

# ─────────────────────────────────────────────────────────────────────────────
# 参数解析
# ─────────────────────────────────────────────────────────────────────────────
DRY_RUN=false
UNINSTALL=false
RESTORE_BACKUP=false

for arg in "$@"; do
    case "$arg" in
        --dry-run)        DRY_RUN=true ;;
        --uninstall)      UNINSTALL=true ;;
        --restore-backup) RESTORE_BACKUP=true ;;
        --help|-h)
            grep '^#' "${BASH_SOURCE[0]}" | sed 's/^# \?//'
            exit 0
            ;;
        *)
            echo "[ERROR] 未知参数: $arg" >&2
            exit 1
            ;;
    esac
done

# ─────────────────────────────────────────────────────────────────────────────
# 工具函数
# ─────────────────────────────────────────────────────────────────────────────
log()  { echo "[install.sh] $*"; }
drylog() { echo "[DRY-RUN]   $*"; }
err()  { echo "[ERROR]     $*" >&2; exit 1; }

# 执行或打印（dry-run 时只打印）
# 用 bash -c 替代 eval，避免含空格路径二次分词问题
run() {
    if $DRY_RUN; then
        drylog "would run: $1"
    else
        bash -c "$1"
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# 依赖检查
# ─────────────────────────────────────────────────────────────────────────────
check_deps() {
    if ! command -v jq &>/dev/null; then
        err "缺少 jq。请先安装：brew install jq"
    fi
    # cargo 只在 dev 模式下必需（tarball 模式跳过编译）
    if [[ "$INSTALL_MODE" == "dev" ]] && ! command -v cargo &>/dev/null; then
        err "缺少 cargo（Rust 工具链）。请先安装：https://rustup.rs"
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# 备份 settings.json（写入前调用）
# ─────────────────────────────────────────────────────────────────────────────
backup_settings() {
    local target="$1"
    if [[ -f "$target" ]]; then
        local ts
        ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        local bak="${target}.bak.${ts}"
        if $DRY_RUN; then
            drylog "would backup $target → $bak"
        else
            cp "$target" "$bak"
            log "备份 $target → $bak"
        fi
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# 安装流程
# ─────────────────────────────────────────────────────────────────────────────
do_install() {
    log "安装模式: $INSTALL_MODE"
    log "项目目录: $PROJECT_DIR"
    log "=========================================="

    # 1. 编译（仅 dev 模式；tarball 模式 binary 已预编译）
    if [[ "$INSTALL_MODE" == "dev" ]]; then
        log "步骤 1/6: cargo build --release --workspace"
        run "cd '$PROJECT_DIR' && cargo build --release --workspace"
    else
        log "步骤 1/6: tarball 模式跳过编译（binary 已预编译）"
    fi

    # 2. 校验二进制存在
    if ! $DRY_RUN; then
        [[ -f "$MCP_SERVER_BIN" ]] || err "二进制不存在: $MCP_SERVER_BIN"
        [[ -f "$HOOK_BIN" ]]       || err "二进制不存在: $HOOK_BIN"
    fi
    log "步骤 2/6: 找到二进制"
    $DRY_RUN && drylog "  $MCP_SERVER_BIN" || log "  $MCP_SERVER_BIN"
    $DRY_RUN && drylog "  $HOOK_BIN"       || log "  $HOOK_BIN"

    # 2.5. 清除 macOS quarantine 扩展属性
    # 浏览器/curl 下载的 tarball 解压后，binary 会带 com.apple.quarantine flag，
    # 导致 Gatekeeper 每次 spawn MCP server 时弹"无法验证开发者"授权框。
    # ad-hoc 签名解决不了这个 —— 必须显式移除 quarantine。
    # tarball 模式才需要清；dev 模式 cargo build 出来的 binary 不带 quarantine（无害）。
    if [[ "$(uname -s)" == "Darwin" ]]; then
        log "步骤 2.5/6: 清除 macOS quarantine 扩展属性"
        run "xattr -dr com.apple.quarantine '$MCP_SERVER_BIN' 2>/dev/null || true"
        run "xattr -dr com.apple.quarantine '$HOOK_BIN' 2>/dev/null || true"
    fi

    # 3. 创建 hooks 目录 + 软链
    log "步骤 3/6: 建立软链到 $HOOKS_DIR"
    run "mkdir -p '$HOOKS_DIR'"
    run "ln -sf '$MCP_SERVER_BIN' '$HOOK_LINK_MCP'"
    run "ln -sf '$HOOK_BIN'       '$HOOK_LINK_HOOK'"
    if ! $DRY_RUN; then
        log "  $HOOK_LINK_MCP → $MCP_SERVER_BIN"
        log "  $HOOK_LINK_HOOK → $HOOK_BIN"
    fi

    # 4. 安装 skill 到 ~/.claude/skills/thesis/
    log "步骤 4/6: 安装 skill 到 $SKILLS_DIR/thesis"
    if [[ -d "$SKILL_SRC" ]]; then
        run "mkdir -p '$SKILLS_DIR'"
        run "rm -rf '$SKILLS_DIR/thesis'"
        run "cp -r '$SKILL_SRC' '$SKILLS_DIR/thesis'"
        if ! $DRY_RUN; then
            log "  $SKILLS_DIR/thesis ← $SKILL_SRC"
        fi
    else
        log "  跳过：skill 源不存在 $SKILL_SRC"
    fi

    # 5. 注册 MCP server 到 ~/.claude.json
    log "步骤 5/6: 注册 MCP server 到 $CLAUDE_JSON"
    register_mcp_server

    # 6. 注册 hooks 到 settings.json
    log "步骤 6/6: 注册 hooks 到 $SETTINGS_JSON"
    register_hooks

    log "=========================================="
    log "安装完成！"
    log ""
    log "验证方式:"
    log "  1. 重启 Claude Code"
    log "  2. 在 Claude Code 输入 /thesis 触发 skill"
    log "  3. 尝试 Write file.docx → 应被 PreToolUse hook 阻断"
    log ""
    log "卸载:"
    log "  bash scripts/install.sh --uninstall"
    log ""
    log "查看 hook 日志（MCP server stderr）:"
    log "  tail -f ~/.claude/projects/<project>/sessions/*.jsonl | jq 'select(.type==\"stderr\")'"
    log ""
    log "直接测试 hook:"
    log "  echo '{\"tool_name\":\"Write\",\"tool_input\":{\"file_path\":\"test.docx\",\"content\":\"\"}}' | $HOOK_LINK_HOOK pre-tool-use"
}

# 注册 MCP server 到 ~/.claude.json
register_mcp_server() {
    if $DRY_RUN; then
        drylog "would add mcpServers[\"$MCP_ENTRY_KEY\"] to $CLAUDE_JSON:"
        drylog "  { \"type\": \"stdio\", \"command\": \"$HOOK_LINK_MCP\", \"args\": [] }"
        return
    fi

    backup_settings "$CLAUDE_JSON"

    # 已存在则跳过（幂等）
    if [[ -f "$CLAUDE_JSON" ]]; then
        local existing
        existing="$(jq -r --arg key "$MCP_ENTRY_KEY" '.mcpServers[$key] // empty' "$CLAUDE_JSON" 2>/dev/null || true)"
        if [[ -n "$existing" ]]; then
            log "  mcpServers[\"$MCP_ENTRY_KEY\"] 已存在，跳过"
            return
        fi
    fi

    # 文件不存在则创建空 JSON
    if [[ ! -f "$CLAUDE_JSON" ]]; then
        echo '{}' > "$CLAUDE_JSON"
    fi

    # 追加 mcpServers 条目
    local tmp
    tmp="$(mktemp)"
    jq --arg key "$MCP_ENTRY_KEY" \
       --arg cmd "$HOOK_LINK_MCP" \
       '.mcpServers[$key] = { "type": "stdio", "command": $cmd, "args": [] }' \
       "$CLAUDE_JSON" > "$tmp"
    mv "$tmp" "$CLAUDE_JSON"
    log "  已添加 mcpServers[\"$MCP_ENTRY_KEY\"]"
}

# 注册 PreToolUse / Stop / PostToolUse hook 到 settings.json
register_hooks() {
    if $DRY_RUN; then
        drylog "would add to $SETTINGS_JSON:"
        drylog "  PreToolUse (matcher: Write|Edit|MultiEdit|NotebookEdit|Bash|Agent):"
        drylog "    $HOOK_LINK_HOOK pre-tool-use"
        drylog "  Stop:"
        drylog "    $HOOK_LINK_HOOK stop"
        drylog "  PostToolUse:"
        drylog "    $HOOK_LINK_HOOK post-tool-use"
        return
    fi

    [[ -f "$SETTINGS_JSON" ]] || err "$SETTINGS_JSON 不存在，请先安装 Claude Code"

    backup_settings "$SETTINGS_JSON"

    local tmp
    tmp="$(mktemp)"

    # 用 jq 幂等地追加三条 hook
    jq \
      --arg hook_bin "$HOOK_LINK_HOOK" \
      '
      # 检查是否已存在 thesis-hook 的某个 hook event 条目
      def has_thesis_hook(event):
        (.hooks[event] // [])
        | any(
            .hooks
            | any(.command != null and (.command | test("thesis-hook")))
          );

      # 追加一个 hook group 到指定 event（如果尚不存在）
      def add_hook(event; matcher; subcmd):
        if has_thesis_hook(event) then .
        else
          .hooks[event] = (
            (.hooks[event] // []) +
            [{
              "matcher": matcher,
              "hooks": [{
                "type": "command",
                "command": ($hook_bin + " " + subcmd),
                "timeout": 10
              }]
            }]
          )
        end;

      add_hook("PreToolUse"; "Write|Edit|MultiEdit|NotebookEdit|Bash|Agent"; "pre-tool-use")
      | add_hook("Stop"; ""; "stop")
      | add_hook("PostToolUse"; "Write|Edit|MultiEdit|NotebookEdit"; "post-tool-use")
      ' \
      "$SETTINGS_JSON" > "$tmp"

    mv "$tmp" "$SETTINGS_JSON"
    log "  已添加 PreToolUse / Stop / PostToolUse hook 条目"
}

# ─────────────────────────────────────────────────────────────────────────────
# 卸载流程
# ─────────────────────────────────────────────────────────────────────────────
do_uninstall() {
    log "开始卸载 thesis-mcp..."
    log "=========================================="

    # 1. 删除软链
    log "步骤 1/4: 移除软链"
    for link in "${HOOKS_DIR}"/thesis-*; do
        if [[ -L "$link" ]]; then
            run "rm '$link'"
            $DRY_RUN || log "  已删除 $link"
        fi
    done
    $DRY_RUN && drylog "would remove ${HOOKS_DIR}/thesis-* symlinks"

    # 2. 删除 skill（v0.1.3 起 install.sh 安装 skill 到 ~/.claude/skills/thesis）
    log "步骤 2/4: 移除 skill ${SKILLS_DIR}/thesis"
    if [[ -d "${SKILLS_DIR}/thesis" ]]; then
        run "rm -rf '${SKILLS_DIR}/thesis'"
        $DRY_RUN || log "  已删除 ${SKILLS_DIR}/thesis"
    else
        log "  跳过：${SKILLS_DIR}/thesis 不存在"
    fi

    # 3. 从 ~/.claude.json 移除 mcpServers 条目
    log "步骤 3/4: 从 $CLAUDE_JSON 移除 mcpServers[\"$MCP_ENTRY_KEY\"]"
    if $DRY_RUN; then
        drylog "would remove mcpServers[\"$MCP_ENTRY_KEY\"] from $CLAUDE_JSON"
    elif [[ -f "$CLAUDE_JSON" ]]; then
        backup_settings "$CLAUDE_JSON"
        local tmp
        tmp="$(mktemp)"
        jq --arg key "$MCP_ENTRY_KEY" 'del(.mcpServers[$key])' "$CLAUDE_JSON" > "$tmp"
        mv "$tmp" "$CLAUDE_JSON"
        log "  已移除 mcpServers[\"$MCP_ENTRY_KEY\"]"
    fi

    # 4. 从 settings.json 移除 thesis-hook 条目
    log "步骤 4/4: 从 $SETTINGS_JSON 移除 thesis-hook 条目"
    if $DRY_RUN; then
        drylog "would remove thesis-hook hook entries from $SETTINGS_JSON"
    elif [[ -f "$SETTINGS_JSON" ]]; then
        backup_settings "$SETTINGS_JSON"
        local tmp
        tmp="$(mktemp)"
        jq '
          .hooks |= (
            to_entries
            | map(
                .value |= map(
                  select(
                    (.hooks | all(
                      .command == null or (.command | test("thesis-hook") | not)
                    ))
                  )
                )
              )
            | from_entries
          )
        ' "$SETTINGS_JSON" > "$tmp"
        mv "$tmp" "$SETTINGS_JSON"
        log "  已移除 thesis-hook 条目"
    fi

    # 可选：还原最近备份
    if $RESTORE_BACKUP; then
        restore_latest_backup
    fi

    log "=========================================="
    log "卸载完成。请重启 Claude Code 使更改生效。"
}

# 还原最新备份
restore_latest_backup() {
    log "步骤额外: 还原最近备份..."
    for target in "$SETTINGS_JSON" "$CLAUDE_JSON"; do
        # 找到最新的 .bak.* 文件
        local latest
        latest="$(ls -1t "${target}.bak."* 2>/dev/null | head -1 || true)"
        if [[ -n "$latest" ]]; then
            if $DRY_RUN; then
                drylog "would restore $target from $latest"
            else
                cp "$latest" "$target"
                log "  已还原 $target ← $latest"
            fi
        else
            log "  无备份文件可还原: $target"
        fi
    done
}

# ─────────────────────────────────────────────────────────────────────────────
# 主入口
# ─────────────────────────────────────────────────────────────────────────────
check_deps

if $UNINSTALL; then
    do_uninstall
else
    do_install
fi
