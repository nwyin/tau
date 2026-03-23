# Fuzzy Edit: End-to-End Model-in-Loop

Phase: 4 | Type: online | Cost: $60-100 | Time: ~8 hours

## What it measures

Task completion rate as a function of edit strategy, using real model calls
on real edit tasks. The definitive test of whether fuzzy matching improves
end-to-end outcomes enough to justify false-positive risk.

## Why it matters for tau

Benchmarks 1-2 (fuzzy-match, fuzzy-false-positive) establish TP/FP rates for
matching strategies in isolation. But the real question is: does fuzzy matching
translate into more tasks completed? A strategy with 5% FP rate might still
win if it recovers enough failures to boost overall success. Conversely,
exact-only might be fine if models rarely produce near-miss edits in practice.

## Prerequisites

- Benchmarks 1-2 completed (narrows candidate strategies)
- `shared/` infrastructure built (TauSession, BenchConfig, Reporter, Verifier)
- Fuzzy matching implemented in tau (at least one candidate strategy from
  Benchmark 1 results)
- edit-bench fixture format understood

## Fixtures

Adapt the edit-bench fixture format: `input/` (mutated file), `expected/`
(original), `prompt.md`, `metadata.json`.

### Fixture sources

1. **Port from edit-bench**: existing fixtures in `~/projects/edit-bench/fixtures/`
   cover Python mutations. Reuse directly.

2. **Generate new fixtures**: use edit-bench's mutation generator for Rust, TS,
   Go. Target diversity across languages and mutation types.

3. **oh-my-pi's react-edit-benchmark**: port fixtures from oh-my-pi for
   JS/TS/React-specific edit tasks.

Target: ~80 fixtures across 4 languages, balanced by difficulty.

### Fixture layout

```
fixtures/
├── swap-logical-001/
│   ├── input/
│   │   └── completion.py
│   ├── expected/
│   │   └── completion.py
│   ├── prompt.md
│   └── metadata.json
├── flip-mutability-002/
│   ├── input/
│   │   └── parser.rs
│   ...
```

## Variants / run matrix

| Variant | Description | Edit strategy |
|---------|-------------|---------------|
| `tau-exact` | Current replace mode, no fuzzy fallback | Exact match only |
| `tau-trimws` | Exact + trailing-whitespace normalization | Minimal fuzzy |
| `tau-fuzzy-{threshold}` | Best candidate from Benchmark 1 | Tuned fuzzy |
| `tau-hashline` | Hashline mode | Line-hash addressing |
| `baseline-opi` | oh-my-pi with fuzzy at default threshold | Reference harness |

80 tasks x 5 variants x 1 run = 400 runs.
Same model (claude-sonnet-4-6), same temperature for all.

## Procedure

```bash
# 1. Generate fixtures (or copy from edit-bench)
uv run python generate.py ~/projects/edit-bench/fixtures -o fixtures/

# 2. Run all variants
uv run python run.py fixtures/ \
    --model claude-sonnet-4-6 \
    --variants tau-exact,tau-trimws,tau-fuzzy-92,tau-hashline,baseline-opi \
    --timeout 180 \
    --concurrency 4 \
    -o results/

# 3. Run single variant for debugging
uv run python run.py fixtures/ \
    --model claude-sonnet-4-6 \
    --variants tau-exact \
    --filter "difficulty=easy" \
    -o results/debug/
```

## Metrics

### Primary

- **Task success rate** per variant: file matches expected after all edits
- **Delta**: success rate improvement of fuzzy variants over exact baseline

### Secondary

- **Edit tool success rate**: fraction of `file_edit` calls that succeed on
  first attempt (higher for fuzzy strategies)
- **Retry overhead**: extra turns caused by edit failures (lower for fuzzy)
- **Token cost per task**: hashline requires re-reads; fuzzy saves retries.
  Net token effect?
- **False edit rate**: edits that "succeeded" but changed wrong location.
  Detected by diff against expected output.
- **Wall-clock time**: end-to-end including retries

### Scoring formula

**Net improvement** = (fuzzy_success_rate - exact_success_rate) -
                      (fuzzy_false_edit_rate * penalty_weight)

penalty_weight = 3 (a false edit costs ~3x a missed edit in practice,
because the model builds on the wrong state).

## Decision it informs

The central question: **does fuzzy matching improve end-to-end scores enough
to justify false-positive risk?**

Sub-questions:
- Hashline re-read cost vs fuzzy retry savings: which is cheaper in tokens?
- Is the simplest fuzzy (trailing-ws only) good enough, or does Levenshtein
  add meaningful value?
- Does the answer differ by language? (Rust's strict formatting may make
  fuzzy less necessary than JS.)

## Architecture

```
fuzzy-e2e/
├── SPEC.md
├── generate.py       # Fixture generation / import from edit-bench
├── run.py            # Runner (uses shared/session, shared/verifier)
├── variants.py       # Variant definitions
├── fixtures/         # Task fixtures
└── results/          # Benchmark output (gitignored)
```

Uses shared infrastructure:
- `TauSession` for session management
- `Verifier` for output comparison
- `BenchConfig` + `TaskResult` + `Reporter`

Estimated LOC: ~400 (run.py: ~250, generate.py: ~100, variants.py: ~50)

The runner closely mirrors edit-bench's `runner.py` but adds variant
iteration and uses tau's shared session infrastructure.
