# Microbenchmarks

Cheap, fast-feedback benchmarks for evaluating harness feature implementations.
Complement coarse benchmarks (terminal-bench, SWE-bench) by isolating individual
harness engineering decisions.

See [TEMPLATE.md](TEMPLATE.md) for shared patterns, fixture formats, and CLI
conventions. Each benchmark has a detailed `SPEC.md` in its directory.

## Benchmarks

| # | Benchmark | Dir | Phase | Type | Cost | Fixtures | Key question |
|---|-----------|-----|-------|------|------|----------|-------------|
| 1 | Fuzzy match | [fuzzy-match/](fuzzy-match/) | 1 | offline | $0 | synthetic | Which strategies recover near-miss edits safely? |
| 2 | Post-edit diagnostics | [post-edit-diagnostics/](post-edit-diagnostics/) | 2 | online | $2-5 | hand-crafted + **mined** | Compiler check vs prompt-only vs full LSP? |
| 3 | Compaction recall | [compaction-recall/](compaction-recall/) | 3 | online | $10-25 | synthetic | Which compaction strategy preserves facts best? |
| 4 | Compaction efficiency | [compaction-efficiency/](compaction-efficiency/) | 3 | online | $8-12 | hand-crafted + **mined** | Where's the compression ratio knee? |
| 5 | Parallel file ops | [parallel-ops/](parallel-ops/) | 2 | online | $2-3 | synthetic | Does parallel tool execution save enough time? |
| 6 | Sub-agent decomposition | [subagent-decomposition/](subagent-decomposition/) | 4 | online | ~$18 | synthetic | Single-agent vs sub-agents vs Hive? |
| 7 | Todo/plan tracking | [todo-tracking/](todo-tracking/) | 3 | online | $5-15 | hand-crafted | Mandatory plan injection vs optional tool? |
| 8 | Coordination routing | [coordination-routing/](coordination-routing/) | 2 | online | $2-6 | hand-crafted | Prompting vs orchestration shape for cross-thread coordination? |

Shared infrastructure: [shared/](shared/) — TauSession, BenchConfig, TaskResult,
Reporter, ResultStore, commit miner. Ported from [edit-bench](~/projects/edit-bench/).

## Fixture sources

### Synthetic (generated)

Programmatic perturbations of real source code. Fast to generate, good for
coverage, but may not reflect real model behavior patterns.

Used by: fuzzy-match (accuracy + adversarial corpora), parallel-ops
(workspace generator), compaction-recall (conversation templates),
subagent-decomposition (handler codebases).

### Hand-crafted

Small, focused tasks written by hand to test specific scenarios. High quality
but labor-intensive and may miss edge cases.

Used by: post-edit-diagnostics (4 refactoring tasks), todo-tracking (3 multi-step
tasks with error injection), compaction-efficiency (5 complexity-graded tasks),
coordination-routing (1 orchestration stress task).

### Mined from open source commits

Real commits from real projects. `shared/miner.py` walks git history, filters
by commit type, and extracts (input, expected, prompt) fixture directories.

```bash
# Mine single-file edit tasks (for compaction-efficiency)
uv run python -m shared.miner edit ~/projects/hive ~/projects/fastapi \
    -o ../compaction-efficiency/fixtures/mined/ --lang python --max-tasks 20

# Mine multi-file refactoring tasks (for post-edit-diagnostics)
uv run python -m shared.miner refactor ~/projects/hive \
    -o ../post-edit-diagnostics/fixtures/mined/ --lang python --max-tasks 10
```

Two mining strategies:
- **edit**: single/few-file changes, <100 LOC diff → compaction-efficiency
- **refactor**: multi-file type/signature propagation, 2-8 files → post-edit-diagnostics

Good repos to mine from: hive (492 commits), irradiate (223 commits),
takeoff-protocol (457 commits), pycg-rs (94 commits, Rust).

Not useful for: fuzzy-match (needs near-miss old_strings from model behavior),
compaction-recall (needs synthetic conversations with planted facts),
parallel-ops (just needs a codebase, not commit history).

## Implementation roadmap

### Phase 1: Zero-cost ($0, 1 day)

Build corpus, run offline. No API spend.

1. Fuzzy match — accuracy + adversarial corpora, 6 matchers

### Phase 2: Cheap A/B tests ($7-16, 1-2 days)

First online benchmarks. `shared/` infrastructure built.

2. Post-edit diagnostics — A/B test of compiler feedback
3. Parallel file operations — sequential vs parallel tool calls
4. Coordination routing — prompt vs pipeline coordination behavior

### Phase 3: Feature builds + evaluation ($20-40, 3-5 days)

Requires building compaction and todo tracking in tau first.

4. Compaction recall + efficiency curve
5. Todo multi-step completion benchmark

### Phase 4: Full model-in-loop (~$18, 2-3 days)

Expensive, run last. Depends on Phase 1-3 narrowing the design space.

6. Sub-agent decomposition — multi-agent coordination

### Cost summary

| Phase | Cost | Time |
|-------|------|------|
| Phase 1 | $0 | 1 day |
| Phase 2 | $7-16 | 1-2 days |
| Phase 3 | $20-40 | 3-5 days |
| Phase 4 | ~$18 | 2-3 days |
| **Total** | **$45-74** | **~2 weeks** |
