#!/usr/bin/env bash
# Run Rust criterion benchmarks across all crates.
# Outputs bencher-format lines to stdout.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

echo "=== Rust Benchmarks ==="
cargo bench --workspace --bench '*' -- --output-format bencher 2>&1
