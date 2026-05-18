#!/usr/bin/env bash
# thesis-mcp 二进制按需拉取
# 作者: Atlas.oi  日期: 2026-05-18
#
# 用途：从 GitHub release 拉取当前平台对应的 thesis-hook + thesis-mcp-server tarball，
#       校验 sha256 后解压到 ${CLAUDE_PLUGIN_ROOT}/bin/<target>/。
#
# 用法：bash fetch-binaries.sh <cargo-target-triple>
#   <cargo-target-triple>: aarch64-apple-darwin / x86_64-apple-darwin /
#                          x86_64-unknown-linux-gnu / aarch64-unknown-linux-gnu /
#                          x86_64-pc-windows-msvc
#
# 依赖：curl、shasum (macOS/Linux) 或 sha256sum (Linux)、tar 或 unzip (Windows)

set -euo pipefail

if [ -z "${CLAUDE_PLUGIN_ROOT:-}" ]; then
    echo "[fetch-binaries] 错误：CLAUDE_PLUGIN_ROOT 未设置" >&2
    exit 1
fi

TARGET="${1:-}"
if [ -z "$TARGET" ]; then
    echo "[fetch-binaries] 用法：$0 <cargo-target-triple>" >&2
    exit 1
fi

# ============================================================
# 从 plugin.json 提取 version（避免硬编码两处版本号）
# ============================================================
PLUGIN_JSON="${CLAUDE_PLUGIN_ROOT}/.claude-plugin/plugin.json"
if [ ! -f "$PLUGIN_JSON" ]; then
    echo "[fetch-binaries] 错误：未找到 $PLUGIN_JSON" >&2
    exit 1
fi

# 简易 JSON 提取（不依赖 jq）：匹配第一个 "version": "x.y.z"
VERSION=$(grep '"version"' "$PLUGIN_JSON" | head -1 | sed -E 's/.*"version"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/')
if [ -z "$VERSION" ]; then
    echo "[fetch-binaries] 错误：无法从 plugin.json 提取 version" >&2
    exit 1
fi

REPO="Atlas-oi/thesis-mcp"

# Windows 用 .zip，其他平台 .tar.gz
if [[ "$TARGET" == *windows* ]]; then
    ARCHIVE="thesis-mcp-${VERSION}-${TARGET}.zip"
else
    ARCHIVE="thesis-mcp-${VERSION}-${TARGET}.tar.gz"
fi

URL="https://github.com/${REPO}/releases/download/v${VERSION}/${ARCHIVE}"
SHA_URL="${URL}.sha256"

DEST_DIR="${CLAUDE_PLUGIN_ROOT}/bin/${TARGET}"
mkdir -p "$DEST_DIR"

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

# ============================================================
# 下载 tarball + sha256
# ============================================================
echo "[fetch-binaries] 下载 $URL" >&2
if ! curl -fLo "${TMP_DIR}/${ARCHIVE}" "$URL"; then
    echo "[fetch-binaries] 错误：下载失败。请检查仓库 $REPO release v$VERSION 是否包含 $ARCHIVE。" >&2
    echo "  若是私有仓库，需先 gh auth login 并设置 GH_TOKEN 环境变量。" >&2
    exit 1
fi

echo "[fetch-binaries] 下载 sha256: $SHA_URL" >&2
if ! curl -fLo "${TMP_DIR}/${ARCHIVE}.sha256" "$SHA_URL"; then
    echo "[fetch-binaries] 错误：未找到 sha256 校验文件" >&2
    exit 1
fi

# ============================================================
# 校验 sha256（macOS 用 shasum，Linux 优先 sha256sum）
# ============================================================
if command -v sha256sum >/dev/null 2>&1; then
    SHA_CMD="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
    SHA_CMD="shasum -a 256"
else
    echo "[fetch-binaries] 错误：未找到 sha256sum 或 shasum 命令" >&2
    exit 1
fi

EXPECTED=$(awk '{print $1}' "${TMP_DIR}/${ARCHIVE}.sha256")
ACTUAL=$($SHA_CMD "${TMP_DIR}/${ARCHIVE}" | awk '{print $1}')

if [ "$EXPECTED" != "$ACTUAL" ]; then
    echo "[fetch-binaries] 错误：sha256 不匹配" >&2
    echo "  expected: $EXPECTED" >&2
    echo "  actual:   $ACTUAL" >&2
    exit 1
fi
echo "[fetch-binaries] sha256 校验通过" >&2

# ============================================================
# 解压到 bin/<target>/
# ============================================================
echo "[fetch-binaries] 解压到 $DEST_DIR" >&2
if [[ "$ARCHIVE" == *.zip ]]; then
    if ! command -v unzip >/dev/null 2>&1; then
        echo "[fetch-binaries] 错误：缺少 unzip 命令" >&2
        exit 1
    fi
    unzip -o "${TMP_DIR}/${ARCHIVE}" -d "$DEST_DIR" >/dev/null
else
    tar -xzf "${TMP_DIR}/${ARCHIVE}" -C "$DEST_DIR"
fi

# 二进制设可执行权限（Windows 上无意义但无害）
chmod +x "$DEST_DIR"/thesis-hook* "$DEST_DIR"/thesis-mcp-server* 2>/dev/null || true

echo "[fetch-binaries] 完成：$DEST_DIR" >&2
