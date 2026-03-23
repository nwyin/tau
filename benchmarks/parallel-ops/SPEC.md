# Sub-agents: Parallel File Operations

Phase: 2 | Type: online | Cost: $2-3 | Time: ~30 minutes

## What it measures

Does running N independent file reads in parallel (within a single turn)
save wall-clock time vs sequential execution? And how much does it affect
token usage?

## Why it matters for tau

tau's architecture delegates orchestration to Hive. The question is whether
**harness-native parallel tool execution** — the "80% case" where independent
tool calls happen in one turn — is worth adding to the agent loop itself.

This is distinct from full sub-agents (Benchmark 8). Parallel tool execution
is a simpler primitive: the model emits N tool calls in one message, the
harness executes them concurrently, and results come back in one response.
Most frontier models already support this via multi-tool-call responses.

## Prerequisites

- `shared/` infrastructure (TauSession, BenchConfig, Reporter)
- Understanding of how the model batches tool calls (model behavior, not
  harness feature — the harness just needs to execute them concurrently)

## Fixtures

### Codebase setup

A synthetic codebase with 10 independent files, each containing isolated
functions. No cross-file dependencies.

```
workspace/
├── src/
│   ├── auth.py          # exports authenticate()
│   ├── billing.py       # exports process_payment()
│   ├── cache.py         # exports invalidate_cache()
│   ├── config.py        # exports load_config()
│   ├── database.py      # exports connect_db()
│   ├── email.py         # exports send_notification()
│   ├── logging.py       # exports setup_logger()
│   ├── metrics.py       # exports track_event()
│   ├── search.py        # exports process_data()     # <-- target
│   └── validation.py    # exports validate_schema()
└── README.md
```

### Task

"Read all 10 files in `src/` and identify which one exports a function
named `process_data`."

This task is deliberately simple — the goal is to measure execution pattern
(parallel vs sequential), not task difficulty.

### Variants for file count scaling

Also test with 5, 15, and 20 files to see how parallelism scales.

## Variants / run matrix

| Variant | Description | How enforced |
|---------|-------------|-------------|
| `sequential` | Model reads file1, responds, reads file2, responds, ... | System prompt: "Read files one at a time" |
| `parallel` | Model reads all files in one batch tool call | System prompt: "Read all files in a single turn" |
| `natural` | No instruction — model decides | Default system prompt |
| `baseline-cc` | Claude Code (which supports parallel tool calls natively) | Different harness |

### Matrix

4 variants x 4 file counts (5, 10, 15, 20) x 10 runs = **160 runs**

Each run is cheap (~1-2 turns, small files), so total cost stays under $3.

## Procedure

```bash
# 1. Generate workspace fixtures
uv run python generate.py --file-counts 5,10,15,20 -o fixtures/

# 2. Run all variants
uv run python run.py fixtures/ \
    --model claude-sonnet-4-6 \
    --variants sequential,parallel,natural \
    --runs 10 \
    -o results/

# 3. Analyze scaling
uv run python analyze.py results/report.json
```

## Metrics

### Primary

- **Wall-clock time**: end-to-end task completion. Parallel should be
  significantly faster for 10+ files.
- **API calls (turns)**: number of LLM round-trips. Sequential: N turns.
  Parallel: 1-2 turns. Natural: somewhere in between.

### Secondary

- **Token count**: input + output across all turns. Parallel may use slightly
  more input tokens (all results in one context) but far fewer output tokens
  (one response vs N responses).
- **Correctness**: did the model correctly identify the target file?
  (Should be ~100% for all variants — this isn't a difficulty test.)
- **Scaling curve**: how do metrics change from 5 -> 10 -> 15 -> 20 files?

### Win criteria

Parallel is worth implementing if:
- Wall-clock time: >= 20% faster than sequential
- Token count: <= 10% more than sequential
- Correctness: no degradation

### Expected results sketch

```
Variant      Files  Avg Time(s)  Avg Turns  Avg Tokens  Correct%
sequential      10       25.0        11         18K       100%
parallel        10        5.2         2         15K       100%
natural         10       12.0         4         16K       100%
```

## Decision it informs

1. **Should tau add parallel tool execution?** If the model naturally batches
   reads when not constrained, the harness just needs to execute concurrently.
   If the model doesn't batch, we need system prompt guidance.

2. **Is this sufficient, or are full sub-agents needed?** If parallel tool
   calls handle the file-reading case, sub-agents (Benchmark 8) may only be
   needed for coordinated multi-step work.

3. **Validates Hive delegation architecture.** If parallel tool calls in the
   harness cover the 80% case, Hive is only needed for the 20% (true
   multi-agent coordination).

## Architecture

```
parallel-ops/
├── SPEC.md
├── generate.py       # Generate workspace fixtures with N files
├── run.py            # Runner with variant-specific system prompts
├── analyze.py        # Scaling analysis
├── fixtures/         # Workspace fixtures (committed — small files)
│   ├── 5-files/
│   ├── 10-files/
│   ├── 15-files/
│   └── 20-files/
└── results/          # Output (gitignored)
```

Uses shared: `TauSession`, `BenchConfig`, `TaskResult`, `Reporter`.

Does NOT use `Verifier` — correctness is scored by checking the model's
text response, not file output.

Estimated LOC: ~300 (generate.py: ~80, run.py: ~150, analyze.py: ~70)
