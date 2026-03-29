#!/usr/bin/env bash
# Run a Python system benchmark suite against a release-built tau binary.
#
# Usage:
#   ./scripts/bench-system.sh [SUITE] [--model MODEL] [--runs N]
#
# Examples:
#   ./scripts/bench-system.sh                          # default: todo-tracking
#   ./scripts/bench-system.sh fuzzy-match
#   ./scripts/bench-system.sh todo-tracking --model claude-sonnet-4-6 --runs 3
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

SUITE="todo-tracking"
MODEL="${MODEL:-gpt-5.4-mini}"
RUNS="${RUNS:-1}"

# Parse args
while [[ $# -gt 0 ]]; do
    case "$1" in
        --model) MODEL="$2"; shift 2 ;;
        --runs)  RUNS="$2"; shift 2 ;;
        -*)      echo "Unknown flag: $1" >&2; exit 1 ;;
        *)       SUITE="$1"; shift ;;
    esac
done

BENCH_DIR="benchmarks/${SUITE}"
if [[ ! -d "$BENCH_DIR" ]]; then
    echo "Error: benchmark suite '$SUITE' not found at $BENCH_DIR" >&2
    echo "Available suites:" >&2
    ls -1 benchmarks/ | grep -v __pycache__ | grep -v shared | grep -v pyproject | grep -v uv.lock | grep -v TEMPLATE | grep -v __init__ | grep -v "\.md$" >&2
    exit 1
fi

if [[ ! -f "$BENCH_DIR/run.py" ]]; then
    echo "Error: no run.py found in $BENCH_DIR" >&2
    exit 1
fi

echo "=== System Benchmark: $SUITE ==="
echo "    model: $MODEL"
echo "    runs:  $RUNS"

# Build tau release binary
echo "--- Building tau (release) ---"
cargo build --release --quiet

# Ensure Python deps
echo "--- Syncing Python deps ---"
cd benchmarks
uv sync --quiet 2>/dev/null || uv sync
cd ..

# Run the benchmark
echo "--- Running $SUITE ---"
RESULTS_DIR="$BENCH_DIR/results"
mkdir -p "$RESULTS_DIR"

cd "$BENCH_DIR"

# Detect if run.py expects a fixtures dir or not
if [[ -d "fixtures" ]]; then
    uv run python run.py fixtures/ \
        --model "$MODEL" \
        --runs "$RUNS" \
        -o results/
else
    uv run python run.py \
        --model "$MODEL" \
        --runs "$RUNS" \
        -o results/
fi

echo ""
echo "Results: $RESULTS_DIR/"
if [[ -f "results/report.md" ]]; then
    echo ""
    cat results/report.md
fi
