# Sub-agents: Decomposition vs Coordination Overhead

Phase: 4 | Type: online | Cost: ~$18 | Time: ~2 hours

## What it measures

At what task complexity does spawning separate agents (with reduced context)
hurt more than it helps? Specifically: the tradeoff between parallelism gains
and coherence loss when splitting a coordinated task across agents.

## Why it matters for tau

tau delegates multi-agent orchestration to Hive. This benchmark tests whether
that architecture is the right call — and whether there's a simpler "harness-
native sub-agent" pattern (like Claude Code's Agent tool) that handles the
middle ground between single-agent and full orchestration.

The fundamental tension: sub-agents don't share context. Agent 2 doesn't know
what Agent 1 extracted. This forces either (a) explicit message passing, (b)
re-discovery via file reads, or (c) orchestrator-managed context sharing.

## Prerequisites

- `shared/` infrastructure (TauSession, BenchConfig, Reporter, Verifier)
- Hive integration working (for the Hive variant)
- `parallel-ops` completed (establishes baseline for parallel tool calls)

## Fixtures

### Task: coordinated refactoring

"Extract common utility functions from 5 callers into a new shared module,
then update all callers to import from the new module."

This task has natural decomposition but requires coordination:
1. **Analysis**: identify common patterns across 5 files
2. **Extract**: create `utils.py` with shared functions
3. **Update callers**: modify each of 5 files to import from `utils.py`

Step 3 depends on step 2 (callers need to know what was extracted and how
it was named). This coordination requirement is what makes sub-agents hard.

### Workspace

```
workspace/
├── src/
│   ├── auth_handler.py      # has duplicate parse_header()
│   ├── api_handler.py       # has duplicate parse_header()
│   ├── webhook_handler.py   # has duplicate parse_header()
│   ├── admin_handler.py     # has duplicate parse_header()
│   └── health_handler.py    # has duplicate parse_header()
├── tests/
│   ├── test_auth.py
│   ├── test_api.py
│   └── ...
└── expected/
    ├── src/
    │   ├── utils.py          # extracted common functions
    │   ├── auth_handler.py   # updated imports
    │   └── ...
    └── tests/                # tests still pass
```

### Task scaling

Create 3 difficulty levels:
- **Easy**: 3 callers, 1 common function, obvious extraction
- **Medium**: 5 callers, 2 common functions, slight variations
- **Hard**: 8 callers, 3 common functions, callers have slight differences
  that require parameter handling

## Variants / run matrix

| Variant | Description | Implementation |
|---------|-------------|----------------|
| `single-agent` | One tau session does everything | Standard tau run |
| `sub-msg` | Agent 1 extracts, sends summary to Agents 2-5 | Harness manages message passing |
| `sub-discover` | Agent 1 extracts, Agents 2-5 discover changes by reading files | No inter-agent communication |
| `hive` | Hive orchestrator coordinates extraction + 5 worker updates | Hive API |

3 difficulties x 4 variants x 3 runs = **36 runs**

## Procedure

```bash
# 1. Generate fixtures at 3 difficulty levels
uv run python generate.py -o fixtures/

# 2. Run all variants
uv run python run.py fixtures/ \
    --model claude-sonnet-4-6 \
    --variants single-agent,sub-msg,sub-discover,hive \
    --runs 3 \
    -o results/

# 3. Analyze coordination overhead
uv run python analyze.py results/report.json
```

## Metrics

### Primary

- **Task success rate**: does final code compile and pass tests?
  Verified by running the test suite and diffing against expected output.
- **Correctness granularity**: out of N callers, how many were correctly
  updated? (Partial credit — a task might correctly update 4/5 callers.)

### Secondary

- **Total tokens**: across all agents in a variant. Sub-agent variants may
  use more tokens due to duplicated context (each agent reads the extracted
  module separately).
- **Wall-clock time**: end-to-end. Sub-agents can parallelize caller updates
  but have coordination overhead.
- **Re-work rate**: did sub-agents redo discovery work? Measured by counting
  redundant `file_read` calls across agents for the same file.
- **Coordination failures**: cases where a sub-agent made changes
  inconsistent with the extraction (e.g., imported a function name that
  Agent 1 named differently).

### Key comparisons

```
Variant         Success%  Avg Tokens  Avg Time(s)  Re-work%  Coord Fail%
single-agent       83%       95K         180          0%          0%
sub-msg            78%      110K         120         10%          5%
sub-discover       65%      130K         140         35%         12%
hive               85%      105K         100          5%          2%
```

If `single-agent` matches or beats sub-agent variants, the coordination
overhead exceeds the parallelism benefit.

## Decision it informs

1. **Whether harness-native sub-agents help on coordinated tasks.** If
   `sub-msg` doesn't beat `single-agent`, the coordination overhead dominates.

2. **Whether Hive-style orchestration is worth it.** If `hive` significantly
   beats all sub-agent variants, tau's architecture (delegate to Hive) is
   vindicated.

3. **The coherence loss question.** If `sub-discover` has high re-work and
   coordination failures, it proves that splitting context hurts — and
   any sub-agent approach needs explicit message passing.

4. **Where's the crossover?** The 3 difficulty levels test whether sub-agents
   become worthwhile as task size grows. Maybe single-agent wins at 3 callers
   but loses at 8.

## Architecture

```
subagent-decomposition/
├── SPEC.md
├── generate.py       # Generate workspace fixtures at 3 difficulties
├── run.py            # Runner with 4 variant execution strategies
├── analyze.py        # Coordination overhead analysis
├── fixtures/         # Workspace fixtures (committed)
│   ├── easy/
│   ├── medium/
│   └── hard/
└── results/          # Output (gitignored)
```

Uses shared: `TauSession`, `BenchConfig`, `TaskResult`, `Reporter`, `Verifier`.

Estimated LOC: ~600 (run.py: ~300, generate.py: ~200, analyze.py: ~100)

### Special runner complexity

The `sub-msg` and `sub-discover` variants require the runner to:
1. Spawn Agent 1 (extraction)
2. Wait for Agent 1 to complete
3. Optionally capture Agent 1's output as message context
4. Spawn Agents 2-N (caller updates) in parallel
5. Wait for all to complete
6. Verify combined output

This multi-session orchestration logic is the most complex runner across
all benchmarks. The Hive variant delegates this to Hive's API.

### Test verification

After all agents complete, run:
```bash
cd workspace && python -m pytest tests/
```

Tests passing = task success (stronger signal than diff-only verification).
