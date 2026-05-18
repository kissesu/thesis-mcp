#!/usr/bin/env bash
# thesis-mcp 二进制启动器
# 作者: Atlas.oi  日期: 2026-05-18
#
# 用途：被 hooks.json / .mcp.json 调用，根据当前平台定位对应二进制，
#       若 binary 不存在则触发 fetch-binaries.sh 首次拉取（GitHub release）。
#
# 用法：bash launcher.sh <binary-name> [args...]
#   <binary-name>: thesis-hook 或 thesis-mcp-server
#
# 依赖：CC 在调用 hook/MCP 时设置 ${CLAUDE_PLUGIN_ROOT} 环境变量。

set -euo pipefail

if [ -z "${CLAUDE_PLUGIN_ROOT:-}" ]; then
    echo "[thesis-mcp launcher] 错误：CLAUDE_PLUGIN_ROOT 未设置，无法定位 plugin 资源" >&2
    exit 1
fi

if [ $# -lt 1 ]; then
    echo "[thesis-mcp launcher] 用法：$0 <binary-name> [args...]" >&2
    exit 1
fi

BIN_NAME="$1"
shift

# ============================================================
# 平台检测 → cargo target triple
# ============================================================
detect_target() {
    local os arch
    os=$(uname -s 2>/dev/null || echo unknown)
    arch=$(uname -m 2>/dev/null || echo unknown)
    case "$os" in
        Darwin)
            case "$arch" in
                arm64) echo "aarch64-apple-darwin" ;;
                x86_64) echo "x86_64-apple-darwin" ;;
                *) echo "unsupported" ;;
            esac
            ;;
        Linux)
            case "$arch" in
                x86_64) echo "x86_64-unknown-linux-gnu" ;;
                aarch64) echo "aarch64-unknown-linux-gnu" ;;
                *) echo "unsupported" ;;
            esac
            ;;
        MINGW*|CYGWIN*|MSYS*)
            echo "x86_64-pc-windows-msvc"
            ;;
        *)
            echo "unsupported"
            ;;
    esac
}

TARGET=$(detect_target)
if [ "$TARGET" = "unsupported" ]; then
    echo "[thesis-mcp launcher] 错误：不支持的平台 $(uname -s)/$(uname -m)" >&2
    echo "  当前支持：macOS arm64/x64、Linux x64/arm64、Windows x64" >&2
    exit 1
fi

# Windows 二进制带 .exe 后缀
EXT=""
if [[ "$TARGET" == *windows* ]]; then
    EXT=".exe"
fi

BIN_PATH="${CLAUDE_PLUGIN_ROOT}/bin/${TARGET}/${BIN_NAME}${EXT}"

# ============================================================
# binary 缺失 → 触发首次拉取
# ============================================================
if [ ! -x "$BIN_PATH" ]; then
    echo "[thesis-mcp launcher] 首次启动：拉取 ${TARGET} 二进制..." >&2
    bash "${CLAUDE_PLUGIN_ROOT}/scripts/fetch-binaries.sh" "$TARGET" >&2
    if [ ! -x "$BIN_PATH" ]; then
        echo "[thesis-mcp launcher] 错误：拉取后仍未找到 $BIN_PATH" >&2
        exit 1
    fi
fi

# ============================================================
# exec binary，stdin/stdout/stderr + 退出码完全透传
# ============================================================
exec "$BIN_PATH" "$@"
