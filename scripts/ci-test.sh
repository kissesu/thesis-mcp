#!/usr/bin/env bash
# scripts/ci-test.sh — thesis-mcp 完整质量门禁（CI 用）
# 依次运行: fmt 检查 → clippy → build → test → 对抗测试摘要
# 任意步骤失败则整体退出非零。
#
# 用法:
#   bash scripts/ci-test.sh
#
# @author Atlas.oi
# @date   2026-05-18

set -euo pipefail

# 切换到项目根目录（无论从哪里调用）
cd "$(dirname "$0")/.."

echo "=========================================="
echo "thesis-mcp CI 质量门禁"
echo "项目目录: $(pwd)"
echo "=========================================="

echo ""
echo "=== fmt ==="
cargo fmt --all -- --check

echo ""
echo "=== clippy ==="
cargo clippy --workspace --all-targets -- -D warnings

echo ""
echo "=== build ==="
cargo build --workspace --all-targets

echo ""
echo "=== test ==="
cargo test --workspace --no-fail-fast

echo ""
echo "=== adversarial summary ==="
# 单独跑对抗测试，提取每条测试的结果摘要
cargo test -p thesis-hook adversarial 2>&1 | grep -E "(test .*adv|test .*adversarial|FAILED|ok|IGNORED)" | head -20 || true

echo ""
echo "=========================================="
echo "=== ALL GREEN ==="
echo "=========================================="
