#!/usr/bin/env bash
# Pre-push hook: runs the AGENTS.md gates locally before push.
# Fmt + clippy + test. Catches issues before CI.
#
# Install: git config core.hooksPath scripts (after symlinking) or
#          ln -sf ../../scripts/pre-push-hook.sh .git/hooks/pre-push
#
# Skip with: git push --no-verify

set -euo pipefail

echo "pre-push: running fmt + clippy + test..."

echo "  cargo fmt --all -- --check"
cargo fmt --all -- --check

echo "  cargo clippy --workspace --all-targets -- -D warnings"
cargo clippy --workspace --all-targets -- -D warnings

echo "  cargo test --workspace"
cargo test --workspace

echo "pre-push: all gates passed ✓"
