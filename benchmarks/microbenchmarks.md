# Microbenchmarks

Cheap, fast-feedback benchmarks for evaluating harness feature implementations.
Complement coarse benchmarks (terminal-bench, SWE-bench) by isolating individual
harness engineering decisions.

See [TEMPLATE.md](TEMPLATE.md) for shared patterns, fixture formats, and CLI
conventions. Each benchmark has a detailed `SPEC.md` in its directory.

## Benchmarks

| # | Benchmark | Dir | Phase | Type | Cost | Fixtures | Key question |
|---|-----------|-----|-------|------|------|----------|-------------|
| 1 | Fuzzy match accuracy | [fuzzy-match/](fuzzy-match/) | 1 | offline | $0 | synthetic | Which strategies recover near-miss edits at zero FP risk? |
| 2 | Fuzzy false-positive audit | [fuzzy-false-positive/](fuzzy-false-positive/) | 1 | offline | $0 | synthetic + real files | Is fuzzy matching safe on repetitive code? |
| 3 | Fuzzy edit e2e | [fuzzy-e2e/](fuzzy-e2e/) | 4 | online | $60-100 | **mined** | Does fuzzy improve end-to-end task completion? |
| 4 | Post-edit diagnostics | [post-edit-diagnostics/](post-edit-diagnostics/) | 2 | online | $2-5 | hand-crafted + **mined** | Compiler check vs prompt-only vs full LSP? |
| 5 | Compaction recall | [compaction-recall/](compaction-recall/) | 3 | online | $10-25 | synthetic | Which compaction strategy preserves facts best? |
| 6 | Compaction efficiency | [compaction-efficiency/](compaction-efficiency/) | 3 | online | $8-12 | hand-crafted + **mined** | Where's the compression ratio knee? |
| 7 | Parallel file ops | [parallel-ops/](parallel-ops/) | 2 | online | $2-3 | synthetic | Does parallel tool execution save enough time? |
| 8 | Sub-agent decomposition | [subagent-decomposition/](subagent-decomposition/) | 4 | online | ~$18 | synthetic | Single-agent vs sub-agents vs Hive? |
| 9 | Todo/plan tracking | [todo-tracking/](todo-tracking/) | 3 | online | $5-15 | hand-crafted | Mandatory plan injection vs optional tool? |

Shared infrastructure: [shared/](shared/) — TauSession, BenchConfig, TaskResult,
Reporter, ResultStore, commit miner. Ported from [edit-bench](~/projects/edit-bench/).

## Fixture sources

### Synthetic (generated)

Programmatic perturbations of real source code. Fast to generate, good for
coverage, but may not reflect real model behavior patterns.

Used by: fuzzy-match, fuzzy-false-positive (adversarial corpus), parallel-ops
(workspace generator), compaction-recall (conversation templates),
subagent-decomposition (handler codebases).

### Hand-crafted

Small, focused tasks written by hand to test specific scenarios. High quality
but labor-intensive and may miss edge cases.

Used by: post-edit-diagnostics (4 refactoring tasks), todo-tracking (3 multi-step
tasks with error injection), compaction-efficiency (5 complexity-graded tasks).

### Mined from open source commits

Real commits from real projects. `shared/miner.py` walks git history, filters
by commit type, and extracts (input, expected, prompt) fixture directories.

```bash
# Mine single-file edit tasks (for fuzzy-e2e)
uv run python -m shared.miner edit ~/projects/hive ~/projects/fastapi \
    -o ../fuzzy-e2e/fixtures/mined/ --lang python --max-tasks 20

# Mine multi-file refactoring tasks (for post-edit-diagnostics)
uv run python -m shared.miner refactor ~/projects/hive \
    -o ../post-edit-diagnostics/fixtures/mined/ --lang python --max-tasks 10
```

Two mining strategies:
- **edit**: single/few-file changes, <100 LOC diff → fuzzy-e2e, compaction-efficiency
- **refactor**: multi-file type/signature propagation, 2-8 files → post-edit-diagnostics

Good repos to mine from: hive (492 commits), irradiate (223 commits),
takeoff-protocol (457 commits), pycg-rs (94 commits, Rust).

Not useful for: fuzzy-match (needs near-miss old_strings from model behavior),
compaction-recall (needs synthetic conversations with planted facts),
parallel-ops (just needs a codebase, not commit history).

## Implementation roadmap

### Phase 1: Zero-cost ($0, 2-3 days)

Build corpus, run offline. No API spend.

1. Fuzzy match accuracy — corpus + matchers (scaffolded)
2. Fuzzy false-positive audit — adversarial corpus

### Phase 2: Cheap A/B tests ($5-10, 1-2 days)

First online benchmarks. `shared/` infrastructure built.

3. Post-edit diagnostics — A/B test of compiler feedback
4. Parallel file operations — sequential vs parallel tool calls

### Phase 3: Feature builds + evaluation ($20-40, 3-5 days)

Requires building compaction and todo tracking in tau first.

5. Build compaction (using `transform_context` hook)
6. Compaction recall + efficiency curve
7. Build todo tracking (~200 LOC)
8. Todo multi-step completion benchmark

### Phase 4: Full model-in-loop ($60-100, 2-3 days)

Expensive, run last. Depends on Phase 1-3 narrowing the design space.

9. Fuzzy edit e2e — adapts edit-bench runner
10. Sub-agent decomposition — multi-agent coordination

### Cost summary

| Phase | Cost | Time |
|-------|------|------|
| Phase 1 | $0 | 2-3 days |
| Phase 2 | $5-10 | 1-2 days |
| Phase 3 | $20-40 | 3-5 days |
| Phase 4 | $60-100 | 2-3 days |
| **Total** | **$85-150** | **~2 weeks** |
