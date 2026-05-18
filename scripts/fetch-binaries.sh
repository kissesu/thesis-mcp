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

REPO="kissesu/thesis-mcp"

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
# 获取 tarball + sha256（多优先级 fetch）
#
# 优先级（按顺序尝试，第一个成功的胜出）：
# 1. ${THESIS_MCP_LOCAL_TARBALL} 环境变量指向的文件（用户明确指定）
# 2. ~/Downloads/${ARCHIVE}（用户从浏览器下载到默认位置）
# 3. gh release download（私有仓库 + 已 gh auth login）
# 4. curl anonymous（公开仓库 + 联网）
# 全失败 → 明确手动下载指引
# ============================================================

TARBALL_PATH="${TMP_DIR}/${ARCHIVE}"
SHA_PATH="${TMP_DIR}/${ARCHIVE}.sha256"
FETCH_METHOD=""

# --- 优先级 1：环境变量指定的本地 tarball ---
if [ -n "${THESIS_MCP_LOCAL_TARBALL:-}" ]; then
    if [ -f "${THESIS_MCP_LOCAL_TARBALL}" ]; then
        echo "[fetch-binaries] 使用 THESIS_MCP_LOCAL_TARBALL: ${THESIS_MCP_LOCAL_TARBALL}" >&2
        cp "${THESIS_MCP_LOCAL_TARBALL}" "$TARBALL_PATH"
        # 同目录找 .sha256（约定）
        if [ -f "${THESIS_MCP_LOCAL_TARBALL}.sha256" ]; then
            cp "${THESIS_MCP_LOCAL_TARBALL}.sha256" "$SHA_PATH"
        fi
        FETCH_METHOD="local-env"
    else
        echo "[fetch-binaries] 错误：THESIS_MCP_LOCAL_TARBALL 指向的文件不存在：${THESIS_MCP_LOCAL_TARBALL}" >&2
        exit 1
    fi
fi

# --- 优先级 2：~/Downloads/${ARCHIVE} ---
if [ -z "$FETCH_METHOD" ] && [ -n "${HOME:-}" ]; then
    DL_TARBALL="${HOME}/Downloads/${ARCHIVE}"
    if [ -f "$DL_TARBALL" ]; then
        echo "[fetch-binaries] 发现本地下载 tarball: $DL_TARBALL" >&2
        cp "$DL_TARBALL" "$TARBALL_PATH"
        if [ -f "${DL_TARBALL}.sha256" ]; then
            cp "${DL_TARBALL}.sha256" "$SHA_PATH"
        fi
        FETCH_METHOD="local-downloads"
    fi
fi

# --- 优先级 3：gh release download（私有仓库友好）---
if [ -z "$FETCH_METHOD" ] && command -v gh >/dev/null 2>&1 \
        && gh auth status >/dev/null 2>&1; then
    echo "[fetch-binaries] 尝试 gh release download (已 gh auth)..." >&2
    if (cd "$TMP_DIR" && gh release download "v$VERSION" \
            --repo "$REPO" \
            --pattern "$ARCHIVE" \
            --pattern "${ARCHIVE}.sha256" \
            --clobber >/dev/null 2>&1); then
        echo "[fetch-binaries] gh release download 成功" >&2
        FETCH_METHOD="gh-release"
    else
        echo "[fetch-binaries] gh download 失败，回退 anonymous curl" >&2
    fi
fi

# --- 优先级 4：curl anonymous ---
if [ -z "$FETCH_METHOD" ]; then
    echo "[fetch-binaries] anonymous curl: $URL" >&2
    if curl -fLo "$TARBALL_PATH" "$URL" 2>/dev/null \
            && curl -fLo "$SHA_PATH" "$SHA_URL" 2>/dev/null; then
        FETCH_METHOD="curl-anonymous"
    fi
fi

# --- 全失败：明确手动下载指引 ---
if [ -z "$FETCH_METHOD" ] || [ ! -f "$TARBALL_PATH" ] || [ ! -f "$SHA_PATH" ]; then
    cat >&2 <<EOF
[fetch-binaries] 错误：所有 fetch 路径均失败

可能原因：
  - 仓库 $REPO 私有但未配 gh auth（gh CLI 缺失 或 未登录）
  - 网络受限（无法访问 github.com）
  - release v$VERSION 不存在或不含 $ARCHIVE

手动救援方案（任选其一）：

  方案 A. 浏览器下载到 ~/Downloads/（最简）
    访问：$URL
    保存为：${HOME:-~}/Downloads/${ARCHIVE}
    同时下载 sha256：$SHA_URL → ${HOME:-~}/Downloads/${ARCHIVE}.sha256
    然后重跑当前命令（或重新触发 plugin install）

  方案 B. 用 THESIS_MCP_LOCAL_TARBALL 指定路径
    下载 tarball 后：
      export THESIS_MCP_LOCAL_TARBALL=/path/to/${ARCHIVE}
    然后重跑

  方案 C. 私有仓库先 gh login
    gh auth login   # 选 HTTPS + browser auth
    然后重跑

EOF
    exit 1
fi

# sha256 可能因优先级 1/2 未提供 → 尝试单独 fetch 一次
if [ ! -f "$SHA_PATH" ]; then
    echo "[fetch-binaries] tarball 已就绪但缺 sha256，尝试单独 fetch..." >&2
    if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
        (cd "$TMP_DIR" && gh release download "v$VERSION" --repo "$REPO" \
            --pattern "${ARCHIVE}.sha256" --clobber >/dev/null 2>&1) || true
    fi
    if [ ! -f "$SHA_PATH" ]; then
        curl -fLo "$SHA_PATH" "$SHA_URL" 2>/dev/null || true
    fi
    if [ ! -f "$SHA_PATH" ]; then
        echo "[fetch-binaries] 警告：sha256 校验文件缺失，跳过校验（不推荐，但 tarball 已就绪）" >&2
        echo "  本地 tarball 来源：$FETCH_METHOD" >&2
        # 写一个空 sha 触发后续 sha 比对失败 → exit 1
        # 或者：跳过校验。为安全考虑应失败。
        echo "[fetch-binaries] 错误：sha256 缺失时拒绝继续（防伪造）。请提供 ${ARCHIVE}.sha256。" >&2
        exit 1
    fi
fi

echo "[fetch-binaries] tarball 来源：$FETCH_METHOD" >&2

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
