#!/usr/bin/env bash
# run.sh — Execute the thread-coordination benchmark and analyze traces.
#
# Usage:
#   ./benchmarks/thread-coordination/run.sh [--model MODEL]
#
# This benchmark tests tau's thread orchestration by having it build
# a multi-component web app using parallel threads. After the run,
# it validates the output AND analyzes the trace to understand how
# threads coordinated.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PROMPT_FILE="$SCRIPT_DIR/prompt.txt"

# Defaults
MODEL="${MODEL:-gpt-5.4-mini}"
MAX_TURNS=30

# Parse args
while [[ $# -gt 0 ]]; do
    case $1 in
        --model) MODEL="$2"; shift 2 ;;
        --max-turns) MAX_TURNS="$2"; shift 2 ;;
        *) echo "Unknown arg: $1"; exit 1 ;;
    esac
done

# Build the agent
echo "=== Building coding-agent ==="
cargo build -p coding-agent --manifest-path "$REPO_ROOT/Cargo.toml" 2>&1 | tail -1
AGENT_BIN="$REPO_ROOT/target/debug/tau"

if [[ ! -x "$AGENT_BIN" ]]; then
    echo "ERROR: coding-agent binary not found at $AGENT_BIN"
    exit 1
fi

# Create isolated workspace
WORKDIR=$(mktemp -d -t tau-thread-coord-XXXXXX)
echo "=== Workspace: $WORKDIR ==="

# Read the prompt
PROMPT=$(cat "$PROMPT_FILE")

# Run the agent
echo "=== Running agent (model=$MODEL, max_turns=$MAX_TURNS) ==="
START_TIME=$(date +%s)

cd "$WORKDIR"
TAU_MAX_TURNS=$MAX_TURNS "$AGENT_BIN" \
    --prompt "$PROMPT" \
    --model "$MODEL" \
    --yolo \
    --stats \
    2>&1 | tee "$WORKDIR/agent-output.log" || true

# Show any errors from the run
if grep -qi "error\|panic\|no api key" "$WORKDIR/agent-output.log" 2>/dev/null; then
    echo ""
    echo "=== Agent errors detected ==="
    grep -i "error\|panic\|no api key" "$WORKDIR/agent-output.log" | head -10
fi

END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))

# --- Scorecard ---
echo ""
echo "============================================"
echo "  THREAD-COORDINATION SCORECARD"
echo "  Model: $MODEL"
echo "  Time: ${ELAPSED}s"
echo "============================================"

PASS=0
FAIL=0
TOTAL=8

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

# Check 1: recipes.db exists with tables
if [[ -f "$WORKDIR/recipes.db" ]] && python3 -c "
import sqlite3
conn = sqlite3.connect('$WORKDIR/recipes.db')
tables = [r[0] for r in conn.execute(\"SELECT name FROM sqlite_master WHERE type='table'\").fetchall()]
assert 'recipes' in tables, f'missing recipes table, have: {tables}'
assert 'ingredients' in tables, f'missing ingredients table'
rows = conn.execute('SELECT count(*) FROM recipes').fetchone()[0]
assert rows >= 8, f'Expected >= 8 recipes, got {rows}'
" 2>/dev/null; then
    check "recipes.db with tables and >= 8 recipes" "1"
else
    check "recipes.db with tables and >= 8 recipes" "0"
fi

# Check 2: ingredients seeded
if [[ -f "$WORKDIR/recipes.db" ]] && python3 -c "
import sqlite3
conn = sqlite3.connect('$WORKDIR/recipes.db')
rows = conn.execute('SELECT count(*) FROM ingredients').fetchone()[0]
assert rows >= 20, f'Expected >= 20 ingredients, got {rows}'
" 2>/dev/null; then
    check "ingredients table seeded (>= 20 rows)" "1"
else
    check "ingredients table seeded (>= 20 rows)" "0"
fi

# Check 3: main app file exists
if [[ -f "$WORKDIR/app.py" ]] || [[ -f "$WORKDIR/main.py" ]]; then
    check "app entry point exists" "1"
else
    check "app entry point exists" "0"
fi

# Check 4: templates exist
if ls "$WORKDIR"/templates/*.html >/dev/null 2>&1; then
    check "HTML templates exist" "1"
else
    check "HTML templates exist" "0"
fi

# Check 5: test file exists
if [[ -f "$WORKDIR/test_app.py" ]] || [[ -f "$WORKDIR/tests/test_app.py" ]]; then
    check "test file exists" "1"
else
    check "test file exists" "0"
fi

# Check 6: venv with deps
if [[ -d "$WORKDIR/.venv" ]] && "$WORKDIR/.venv/bin/python" -c "import fastapi; import pytest" 2>/dev/null; then
    check "fastapi and pytest importable" "1"
else
    check "fastapi and pytest importable" "0"
fi

# Check 7: API endpoints work
TEST_FILE=$(find "$WORKDIR" -name "test_app.py" -type f 2>/dev/null | head -1)
if [[ -n "$TEST_FILE" ]]; then
    cd "$WORKDIR"
    if uv run python -m pytest "$TEST_FILE" -v --tb=short > "$WORKDIR/pytest-output.log" 2>&1; then
        check "pytest passes" "1"
    else
        check "pytest passes" "0"
        echo "    pytest output (last 20 lines):"
        tail -20 "$WORKDIR/pytest-output.log" | sed 's/^/    /'
    fi
else
    check "pytest passes" "0"
fi

# Check 8: Multiple cuisines
if [[ -f "$WORKDIR/recipes.db" ]] && python3 -c "
import sqlite3
conn = sqlite3.connect('$WORKDIR/recipes.db')
cuisines = conn.execute('SELECT DISTINCT cuisine FROM recipes').fetchall()
assert len(cuisines) >= 3, f'Expected >= 3 cuisines, got {len(cuisines)}: {cuisines}'
" 2>/dev/null; then
    check ">= 3 distinct cuisines" "1"
else
    check ">= 3 distinct cuisines" "0"
fi

echo "--------------------------------------------"
echo "  Result: $PASS/$TOTAL passed"
echo "  Workspace: $WORKDIR"
echo "============================================"

# --- Trace Analysis ---
echo ""
echo "=== Trace Analysis ==="

# Find the trace directory (most recent under ~/.tau/traces/)
TRACE_DIR=$(ls -td ~/.tau/traces/*/ 2>/dev/null | head -1)

if [[ -z "$TRACE_DIR" ]] || [[ ! -f "$TRACE_DIR/trace.jsonl" ]]; then
    echo "  No trace found — skipping trace analysis"
else
    TRACE_FILE="$TRACE_DIR/trace.jsonl"
    echo "  Trace: $TRACE_FILE"
    echo ""

    # Event type distribution
    echo "  Event distribution:"
    jq -r '.event' "$TRACE_FILE" | sort | uniq -c | sort -rn | sed 's/^/    /'
    echo ""

    # Thread activity
    THREAD_COUNT=$(jq -r 'select(.event == "thread_start") | .alias' "$TRACE_FILE" 2>/dev/null | wc -l | tr -d ' ')
    echo "  Threads spawned: $THREAD_COUNT"
    if [[ "$THREAD_COUNT" -gt 0 ]]; then
        echo "  Thread aliases:"
        jq -r 'select(.event == "thread_start") | "    \(.alias) — \(.task[:80])"' "$TRACE_FILE" 2>/dev/null
        echo ""
        echo "  Thread outcomes:"
        jq -r 'select(.event == "thread_end") | "    \(.alias): \(.outcome) (\(.duration_ms)ms)"' "$TRACE_FILE" 2>/dev/null
    fi
    echo ""

    # Episode routing
    EPISODE_COUNT=$(jq -r 'select(.event == "episode_inject")' "$TRACE_FILE" 2>/dev/null | wc -l | tr -d ' ')
    echo "  Episode injections: $EPISODE_COUNT"
    if [[ "$EPISODE_COUNT" -gt 0 ]]; then
        jq -r 'select(.event == "episode_inject") | "    \(.source_aliases | join(",")) → \(.target_alias)"' "$TRACE_FILE" 2>/dev/null
    fi
    echo ""

    # Document operations
    DOC_COUNT=$(jq -r 'select(.event == "document_op")' "$TRACE_FILE" 2>/dev/null | wc -l | tr -d ' ')
    echo "  Document operations: $DOC_COUNT"
    if [[ "$DOC_COUNT" -gt 0 ]]; then
        jq -r 'select(.event == "document_op") | "    \(.op) \(.name) (\(.content | length) chars)"' "$TRACE_FILE" 2>/dev/null
    fi
    echo ""

    # Query calls
    QUERY_COUNT=$(jq -r 'select(.event == "query_start")' "$TRACE_FILE" 2>/dev/null | wc -l | tr -d ' ')
    echo "  Query calls: $QUERY_COUNT"
    if [[ "$QUERY_COUNT" -gt 0 ]]; then
        jq -r 'select(.event == "query_start") | "    \(.query_id): \(.prompt[:60])..."' "$TRACE_FILE" 2>/dev/null
    fi
    echo ""

    # Evidence citations
    EVIDENCE_COUNT=$(jq -r 'select(.event == "evidence_cite")' "$TRACE_FILE" 2>/dev/null | wc -l | tr -d ' ')
    echo "  Evidence citations: $EVIDENCE_COUNT"

    # Context compactions
    COMPACT_COUNT=$(jq -r 'select(.event == "context_compact")' "$TRACE_FILE" 2>/dev/null | wc -l | tr -d ' ')
    echo "  Context compactions: $COMPACT_COUNT"

    # Copy trace to results
    mkdir -p "$SCRIPT_DIR/results"
    RESULT_ID=$(date +%Y%m%d-%H%M%S)
    cp "$TRACE_FILE" "$SCRIPT_DIR/results/trace-$RESULT_ID.jsonl"
    echo ""
    echo "  Trace saved: results/trace-$RESULT_ID.jsonl"
fi

# Print agent stats
if [[ -f "$WORKDIR/agent-stderr.log" ]]; then
    if grep -q "Agent Statistics" "$WORKDIR/agent-stderr.log" 2>/dev/null; then
        echo ""
        echo "=== Agent Stats ==="
        cat "$WORKDIR/agent-stderr.log"
    fi
fi

echo ""
echo "  Workspace preserved at: $WORKDIR"

# Exit code reflects pass/fail
if [[ "$PASS" -eq "$TOTAL" ]]; then
    exit 0
else
    exit 1
fi
