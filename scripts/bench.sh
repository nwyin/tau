#!/usr/bin/env bash
# Unified benchmark runner for tau.
#
# Runs Rust criterion benchmarks and/or Python system benchmarks,
# merges results into a JSON file tagged with commit SHA.
#
# Usage:
#   ./scripts/bench.sh                    # run everything
#   ./scripts/bench.sh --rust             # rust micro-benchmarks only
#   ./scripts/bench.sh --system           # python system benchmarks only
#   ./scripts/bench.sh --system --suite todo-tracking --model claude-sonnet-4-6
#   ./scripts/bench.sh --compare results/bench-previous.json
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

RUN_RUST=false
RUN_SYSTEM=false
SUITE="todo-tracking"
MODEL="${MODEL:-gpt-5.4-mini}"
RUNS="${RUNS:-1}"
COMPARE=""

# Parse args
while [[ $# -gt 0 ]]; do
    case "$1" in
        --rust)    RUN_RUST=true; shift ;;
        --system)  RUN_SYSTEM=true; shift ;;
        --suite)   SUITE="$2"; shift 2 ;;
        --model)   MODEL="$2"; shift 2 ;;
        --runs)    RUNS="$2"; shift 2 ;;
        --compare) COMPARE="$2"; shift 2 ;;
        -h|--help)
            echo "Usage: $0 [--rust] [--system] [--suite SUITE] [--model MODEL] [--runs N] [--compare FILE]"
            exit 0
            ;;
        *) echo "Unknown arg: $1" >&2; exit 1 ;;
    esac
done

# Default: run both if neither specified
if ! $RUN_RUST && ! $RUN_SYSTEM; then
    RUN_RUST=true
    RUN_SYSTEM=true
fi

COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
RESULTS_DIR="results"
mkdir -p "$RESULTS_DIR"
OUTFILE="$RESULTS_DIR/bench-${COMMIT}-$(date +%s).json"

echo "tau benchmark runner"
echo "  commit:    $COMMIT"
echo "  timestamp: $TIMESTAMP"
echo ""

# Initialize result JSON
RUST_JSON="{}"
SYSTEM_JSON="{}"

# --- Rust benchmarks ---
if $RUN_RUST; then
    echo "========================================"
    RUST_OUTPUT=$(bash scripts/bench-rust.sh 2>&1) || true
    echo "$RUST_OUTPUT"

    # Parse bencher output into JSON
    RUST_JSON=$(echo "$RUST_OUTPUT" | grep "^test " | python3 -c "
import sys, json
results = {}
for line in sys.stdin:
    line = line.strip()
    if not line.startswith('test '):
        continue
    parts = line.split()
    # format: test name ... bench: N ns/iter (+/- V)
    name = parts[1]
    try:
        bench_idx = parts.index('bench:')
        ns = int(parts[bench_idx + 1].replace(',', ''))
        variance = int(parts[bench_idx + 4].replace(',', '').rstrip(')'))
        results[name] = {'ns': ns, 'variance': variance}
    except (ValueError, IndexError):
        pass
print(json.dumps(results))
" 2>/dev/null || echo "{}")
    echo ""
fi

# --- System benchmarks ---
if $RUN_SYSTEM; then
    echo "========================================"
    MODEL="$MODEL" RUNS="$RUNS" bash scripts/bench-system.sh "$SUITE" --model "$MODEL" --runs "$RUNS" || true

    # Read the Python report JSON if it exists
    REPORT="benchmarks/${SUITE}/results/report.json"
    if [[ -f "$REPORT" ]]; then
        SYSTEM_JSON=$(python3 -c "
import json, sys
with open('$REPORT') as f:
    data = json.load(f)
summary = data.get('summary', data)
out = {
    'suite': '$SUITE',
    'model': '$MODEL',
    'runs': $RUNS,
}
# Extract key metrics if available
if isinstance(summary, dict):
    for key in ['pass_rate', 'avg_tokens_in', 'avg_tokens_out', 'avg_wall_clock_ms', 'total_tasks', 'passed']:
        if key in summary:
            out[key] = summary[key]
print(json.dumps(out))
" 2>/dev/null || echo "{\"suite\": \"$SUITE\", \"model\": \"$MODEL\"}")
    fi
    echo ""
fi

# --- Merge results ---
python3 -c "
import json
result = {
    'commit': '$COMMIT',
    'timestamp': '$TIMESTAMP',
    'rust': $RUST_JSON,
    'system': $SYSTEM_JSON,
}
with open('$OUTFILE', 'w') as f:
    json.dump(result, f, indent=2)
print(json.dumps(result, indent=2))
"

echo ""
echo "Results saved to: $OUTFILE"

# --- Compare ---
if [[ -n "$COMPARE" && -f "$COMPARE" ]]; then
    echo ""
    echo "========================================"
    echo "Comparing against: $COMPARE"
    python3 -c "
import json, sys

with open('$COMPARE') as f:
    old = json.load(f)
with open('$OUTFILE') as f:
    new = json.load(f)

print(f\"  old commit: {old.get('commit', '?')}  |  new commit: {new.get('commit', '?')}\")
print()

# Rust comparison
if old.get('rust') and new.get('rust'):
    print('Rust benchmarks:')
    for name in sorted(set(list(old['rust'].keys()) + list(new['rust'].keys()))):
        o = old['rust'].get(name, {}).get('ns', 0)
        n = new['rust'].get(name, {}).get('ns', 0)
        if o > 0 and n > 0:
            pct = ((n - o) / o) * 100
            marker = '!!!' if abs(pct) > 10 else ''
            print(f'  {name}: {o:,} -> {n:,} ns ({pct:+.1f}%) {marker}')

# System comparison
os = old.get('system', {})
ns = new.get('system', {})
if os and ns:
    print()
    print('System benchmarks:')
    for key in ['pass_rate', 'avg_tokens_in', 'avg_tokens_out', 'avg_wall_clock_ms']:
        ov = os.get(key)
        nv = ns.get(key)
        if ov is not None and nv is not None:
            if isinstance(ov, float):
                print(f'  {key}: {ov:.3f} -> {nv:.3f}')
            else:
                print(f'  {key}: {ov:,} -> {nv:,}')
"
fi
