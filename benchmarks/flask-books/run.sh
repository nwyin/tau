#!/usr/bin/env bash
# run.sh — Execute the flask-books eval against the tau coding-agent.
#
# Usage:
#   ./evals/flask-books/run.sh [--model MODEL] [--provider PROVIDER]
#
# Requirements:
#   - cargo build -p coding-agent (or cargo install --path coding-agent)
#   - An API key for the chosen provider (OPENAI_API_KEY or ANTHROPIC_API_KEY)
#   - Python 3.x available on PATH
#
# The script:
#   1. Creates a temp working directory
#   2. Runs the coding-agent in --prompt mode with the eval prompt
#   3. Validates the output (files exist, tests pass)
#   4. Prints a scorecard and cleans up

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PROMPT_FILE="$SCRIPT_DIR/prompt.txt"

# Defaults
MODEL="${MODEL:-gpt-4o}"
PROVIDER=""
STATS_FLAG=""
MAX_TURNS=15

# Parse args
while [[ $# -gt 0 ]]; do
    case $1 in
        --model) MODEL="$2"; shift 2 ;;
        --provider) PROVIDER="$2"; shift 2 ;;
        --max-turns) MAX_TURNS="$2"; shift 2 ;;
        *) echo "Unknown arg: $1"; exit 1 ;;
    esac
done

# Build the agent
echo "=== Building coding-agent ==="
cargo build -p coding-agent --manifest-path "$REPO_ROOT/Cargo.toml" 2>&1 | tail -1
AGENT_BIN="$REPO_ROOT/target/debug/coding-agent"

if [[ ! -x "$AGENT_BIN" ]]; then
    echo "ERROR: coding-agent binary not found at $AGENT_BIN"
    exit 1
fi

# Create isolated workspace
WORKDIR=$(mktemp -d -t tau-eval-flask-books-XXXXXX)
echo "=== Workspace: $WORKDIR ==="

# Read the prompt
PROMPT=$(cat "$PROMPT_FILE")

# Run the agent
echo "=== Running agent (model=$MODEL) ==="
START_TIME=$(date +%s)

cd "$WORKDIR"
"$AGENT_BIN" --prompt "$PROMPT" --model "$MODEL" --stats 2>"$WORKDIR/agent-stderr.log" || true

END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))

# --- Scorecard ---
echo ""
echo "============================================"
echo "  FLASK-BOOKS EVAL SCORECARD"
echo "  Model: $MODEL"
echo "  Time: ${ELAPSED}s"
echo "============================================"

PASS=0
FAIL=0
TOTAL=6

check() {
    local desc="$1"
    local result="$2"
    if [[ "$result" == "1" ]]; then
        echo "  [PASS] $desc"
        PASS=$((PASS + 1))
    else
        echo "  [FAIL] $desc"
        FAIL=$((FAIL + 1))
    fi
}

# Check 1: books.db exists and has data
if [[ -f "$WORKDIR/books.db" ]] && python3 -c "
import sqlite3
conn = sqlite3.connect('$WORKDIR/books.db')
rows = conn.execute('SELECT count(*) FROM books').fetchone()[0]
assert rows >= 5, f'Expected >= 5 rows, got {rows}'
" 2>/dev/null; then
    check "books.db exists with >= 5 rows" "1"
else
    check "books.db exists with >= 5 rows" "0"
fi

# Check 2: app.py exists
if [[ -f "$WORKDIR/app.py" ]]; then
    check "app.py exists" "1"
else
    check "app.py exists" "0"
fi

# Check 3: templates/books.html exists
if [[ -f "$WORKDIR/templates/books.html" ]]; then
    check "templates/books.html exists" "1"
else
    check "templates/books.html exists" "0"
fi

# Check 4: test_app.py exists
if [[ -f "$WORKDIR/test_app.py" ]]; then
    check "test_app.py exists" "1"
else
    check "test_app.py exists" "0"
fi

# Check 5: Flask and pytest importable
if python3 -c "import flask; import pytest" 2>/dev/null; then
    check "flask and pytest importable" "1"
else
    check "flask and pytest importable" "0"
fi

# Check 6: Tests pass
if [[ -f "$WORKDIR/test_app.py" ]] && [[ -f "$WORKDIR/app.py" ]]; then
    cd "$WORKDIR"
    if python3 -m pytest test_app.py -v --tb=short > "$WORKDIR/pytest-output.log" 2>&1; then
        check "pytest passes" "1"
    else
        check "pytest passes" "0"
        echo "    pytest output:"
        tail -20 "$WORKDIR/pytest-output.log" | sed 's/^/    /'
    fi
else
    check "pytest passes" "0"
fi

echo "--------------------------------------------"
echo "  Result: $PASS/$TOTAL passed"
echo "  Workspace: $WORKDIR"
echo "============================================"

# Print agent stats if available
if [[ -f "$WORKDIR/agent-stderr.log" ]]; then
    if grep -q "Agent Statistics" "$WORKDIR/agent-stderr.log" 2>/dev/null; then
        echo ""
        echo "=== Agent Stats ==="
        cat "$WORKDIR/agent-stderr.log"
    fi
fi

# Exit code reflects pass/fail
if [[ "$PASS" -eq "$TOTAL" ]]; then
    exit 0
else
    exit 1
fi
