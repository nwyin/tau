# Compaction: Token Efficiency Curve

Phase: 3 | Type: online | Cost: $8-12 | Time: ~2-3 hours

## What it measures

The Pareto frontier of token savings vs task success rate across compaction
strategies and compression ratios. Finds the "knee" — where more aggressive
compaction starts hurting outcomes.

## Why it matters for tau

Compaction is a tradeoff: compress too little and you hit context limits;
compress too much and the model loses critical context. Every harness picks
a threshold (codex: ~80%, opencode: dynamic, oh-my-pi: 70%) but none publish
data on how they chose it. This benchmark generates that data for tau.

Paired with `compaction-recall` (which tests information preservation in
isolation), this benchmark tests whether information loss actually degrades
task completion.

## Prerequisites

- Compaction built in tau (at least truncation + observation masking)
- `compaction-recall` completed (establishes which strategy preserves info best)
- `shared/` infrastructure

## Fixtures

5 representative coding tasks at different complexity levels:

| Task | Turns | ~Tokens | Complexity |
|------|-------|---------|-----------|
| Fix a typo bug | 8-12 | ~20K | Simple |
| Add a CLI flag | 15-20 | ~35K | Simple |
| Refactor extract function | 25-35 | ~60K | Medium |
| Multi-file API change | 35-50 | ~80K | Medium |
| Full feature implementation | 50-70 | ~120K | Complex |

Tasks are chosen so that:
- Simple tasks should succeed regardless of compaction strategy
- Complex tasks should be sensitive to compaction quality
- The spread covers tau's practical usage range

### Fixture format

Same as `post-edit-diagnostics`: `input/`, `expected/`, `prompt.md`.

For complex tasks, `expected/` may contain multiple files. The prompt
describes the full task; the model works through it naturally, hitting
compaction thresholds during execution.

## Variants / run matrix

Two dimensions: compaction strategy x compression aggressiveness.

### Strategies

| Strategy | Description |
|----------|-------------|
| `none` | Full history, no compaction (baseline) |
| `truncation` | Drop oldest turns |
| `observation-mask` | Replace old tool outputs with `[omitted]` |
| `llm-summary` | LLM-generated structured summary |
| `progressive` | mask -> prune -> summarize as pressure increases |

### Compression targets

For each strategy (except `none`), run at different compression ratios:

| Level | Target | Description |
|-------|--------|-------------|
| `conservative` | Keep 60% of tokens | Light compression |
| `moderate` | Keep 40% of tokens | Medium compression |
| `aggressive` | Keep 20% of tokens | Heavy compression |

### Full matrix

5 tasks x (1 none + 4 strategies x 3 levels) x 3 runs = **195 runs**

This is expensive. Practical reduction: skip `aggressive` for `truncation`
(known to be destructive) and skip `conservative` for `llm-summary`
(not worth the LLM cost for light compression).

Reduced: 5 tasks x 10 configs x 3 runs = **150 runs**

## Procedure

```bash
# 1. Fixtures (hand-crafted or derived from existing benchmarks)
# Small set — committed to repo

# 2. Run full matrix
uv run python run.py fixtures/ \
    --model claude-sonnet-4-6 \
    --strategies none,truncation,observation-mask,llm-summary,progressive \
    --compression conservative,moderate,aggressive \
    --runs 3 \
    --concurrency 2 \
    -o results/

# 3. Generate Pareto plot data
uv run python analyze.py results/report.json -o results/pareto.json
```

## Metrics

### Primary (plotted)

Each data point on the Pareto plot:
- **X axis**: compression ratio (tokens_after / tokens_before). 0.0 = max
  compression, 1.0 = no compression.
- **Y axis**: task success rate across all tasks at that config.

The **Pareto frontier** connects the configs that are not dominated (no other
config has both better success rate and better compression).

### Secondary

- **Per-complexity breakdown**: do simple tasks tolerate aggressive compaction
  while complex tasks don't?
- **Strategy comparison at fixed compression**: at 40% retention, which
  strategy has highest success rate?
- **Cost-adjusted success**: (success_rate * tasks) / total_api_cost. Higher
  is more cost-efficient.
- **Compaction overhead**: time and tokens spent on compaction itself (LLM
  summary cost vs truncation cost of 0).

### Target output

```
Strategy            Compression  Success%  Avg Tokens  Overhead
none                     1.00       90%       85K        0ms
truncation/conservative  0.60       85%       52K        0ms
truncation/moderate      0.40       70%       35K        0ms
obs-mask/conservative    0.55       88%       48K        0ms
obs-mask/moderate        0.40       82%       35K        0ms
obs-mask/aggressive      0.25       65%       22K        0ms
llm-summary/moderate     0.35       85%       30K      8500ms
llm-summary/aggressive   0.20       78%       18K      8500ms
progressive/moderate     0.40       87%       35K      4200ms
progressive/aggressive   0.25       75%       22K      4200ms
```

The "knee" in this example is around 0.40 compression ratio — below that,
success drops sharply.

## Decision it informs

1. **Default compaction threshold.** The knee of the Pareto curve tells us
   when to trigger compaction (e.g., at 80% context -> compress to 40%).

2. **Strategy selection.** If observation masking matches LLM summary at the
   same compression ratio, skip the LLM cost.

3. **Complexity-adjusted policy.** Maybe simple tasks get aggressive
   compaction, complex tasks get conservative. The per-complexity breakdown
   answers this.

4. **Model-specific tuning.** If budget allows, repeat with Haiku: do
   smaller models need gentler compaction?

## Architecture

```
compaction-efficiency/
├── SPEC.md
├── run.py            # Runner with strategy x compression matrix
├── analyze.py        # Pareto frontier analysis + plotting
├── fixtures/         # 5 coding tasks (committed)
│   └── {task}/
│       ├── input/
│       ├── expected/
│       └── prompt.md
└── results/          # Output (gitignored)
```

Uses shared: `TauSession`, `BenchConfig`, `TaskResult`, `Reporter`, `Verifier`.

Estimated LOC: ~400 (run.py: ~200, analyze.py: ~150, fixtures: hand-crafted)

### Special considerations

- **Concurrency limited**: compaction benchmarks are memory-intensive (large
  contexts). Run at concurrency=2 max.
- **Determinism**: compaction triggers depend on exact token counts, which
  vary by run. The runner should log actual trigger points.
- **Cost control**: the `none` baseline at full context is expensive for
  complex tasks. Run it once (not 3x) for cost control.
