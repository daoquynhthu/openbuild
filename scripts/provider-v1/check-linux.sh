#!/usr/bin/env bash
set -euo pipefail
echo "=== Provider V1: Linux Gate ==="
echo ""
echo "--- cargo fmt ---"
cargo fmt --all -- --check
echo ""
echo "--- cargo check ---"
cargo check --workspace --all-targets --locked
echo ""
echo "--- cargo clippy ---"
cargo clippy --workspace --all-targets --locked -- -D warnings
echo ""
echo "=== All checks passed ==="
