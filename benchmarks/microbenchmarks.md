# Microbenchmarks

Cheap, fast-feedback benchmarks for evaluating harness feature implementations.
Complement coarse benchmarks (terminal-bench, SWE-bench) by isolating individual
harness engineering decisions.

See [TEMPLATE.md](TEMPLATE.md) for shared patterns, fixture formats, and CLI
conventions. Each benchmark has a detailed `SPEC.md` in its directory.

## Benchmarks

| # | Benchmark | Dir | Phase | Type | Cost | Key question |
|---|-----------|-----|-------|------|------|-------------|
| 1 | Fuzzy match accuracy | [fuzzy-match/](fuzzy-match/) | 1 | offline | $0 | Which strategies recover near-miss edits at zero FP risk? |
| 2 | Fuzzy false-positive audit | [fuzzy-false-positive/](fuzzy-false-positive/) | 1 | offline | $0 | Is fuzzy matching safe on repetitive code? |
| 3 | Fuzzy edit e2e | [fuzzy-e2e/](fuzzy-e2e/) | 4 | online | $60-100 | Does fuzzy improve end-to-end task completion? |
| 4 | Post-edit diagnostics | [post-edit-diagnostics/](post-edit-diagnostics/) | 2 | online | $2-5 | Compiler check vs prompt-only vs full LSP? |
| 5 | Compaction recall | [compaction-recall/](compaction-recall/) | 3 | online | $10-25 | Which compaction strategy preserves facts best? |
| 6 | Compaction efficiency | [compaction-efficiency/](compaction-efficiency/) | 3 | online | $8-12 | Where's the compression ratio knee? |
| 7 | Parallel file ops | [parallel-ops/](parallel-ops/) | 2 | online | $2-3 | Does parallel tool execution save enough time? |
| 8 | Sub-agent decomposition | [subagent-decomposition/](subagent-decomposition/) | 4 | online | ~$18 | Single-agent vs sub-agents vs Hive? |
| 9 | Todo/plan tracking | [todo-tracking/](todo-tracking/) | 3 | online | $5-15 | Mandatory plan injection vs optional tool? |

Shared infrastructure: [shared/](shared/) — TauSession, BenchConfig, TaskResult,
Reporter, Verifier. Ported from [edit-bench](~/projects/edit-bench/).

## Implementation roadmap

### Phase 1: Zero-cost ($0, 2-3 days)

Build corpus, run offline. No API spend.

1. Fuzzy match accuracy — corpus + matchers (scaffolded)
2. Fuzzy false-positive audit — adversarial corpus

### Phase 2: Cheap A/B tests ($5-10, 1-2 days)

First online benchmarks. Build `shared/` infrastructure first.

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
