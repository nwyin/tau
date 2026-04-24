# Lox Interpreter Stress Test Case Study

**Issue:** #13, "Stress test: Adaptive orchestration builds 252/252 Lox interpreter"
**Status:** case study from trace analysis and issue discussion
**Primary result:** tau built a complete Rust Lox interpreter from scratch and passed
252/252 tests.

This case study records what the run demonstrated about tau's adaptive
orchestration model: workqueue-driven supervision, worker/verifier threads,
checkpoint decisions, shared documents, episode injection, and the handoff from
structured orchestration back to the main agent for the final long tail.

Related local references:

- `README.md` summarizes tau's thread/query/document/py_repl orchestration surface.
- `docs/design-orchestration.md` gives the original py_repl and orchestration design.
- `docs/trace-analysis.md` documents the JSONL event types used in this analysis.
- `coding-agent/prompts/orchestration/overview.md` defines when to use threads,
  queries, and adaptive checkpoints.
- `coding-agent/prompts/orchestration/workflows/supervised.md` contains the
  adaptive supervised-loop template.
- `coding-agent/prompts/orchestration/documents.md` describes shared virtual documents.
- `coding-agent/src/tools/thread.rs`, `coding-agent/src/tools/document.rs`,
  `coding-agent/src/tools/query.rs`, `coding-agent/src/tools/py_repl.rs`, and
  `coding-agent/src/tools/worktree.rs` implement the main orchestration tools.
- `agent/src/orchestrator.rs` stores reusable thread episodes and virtual documents.
- `tools/tau-trace/README.md` documents trace inspection tooling.

## Scenario

The stress test started from an empty Rust project scaffold for the Lox language
from *Crafting Interpreters*. The test project supplied:

- a minimal `Cargo.toml` and `src/main.rs`
- `_reference/test/`, a sparse checkout of the upstream Crafting Interpreters
  test suite
- `test_runner.py`, which validates expected output comments
- `.tau/workqueue.json`, an initial 8-item phased workqueue
- a run prompt instructing tau to use py_repl with the adaptive supervised loop

The scratch Lox project and trace artifacts were not committed to this repository;
the durable data here comes from the issue's trace analysis and follow-up notes.

The run prompt asked tau to:

1. Respect workqueue dependencies.
2. Spawn an isolated worker thread for each item with
   `worktree=True`, `worktree_include=["_reference"]`, and `tools=["full"]`.
3. Spawn a verifier thread based on the worker worktree with `episodes=[worker]`.
4. Merge verified work with `tau.merge(verifier_alias)`.
5. After each failed phase, checkpoint the actual project state with
   `cargo build` and `python3 test_runner.py --summary`.
6. Use `tau.query(..., model="reasoning")` to choose RETRY, SPLIT, SKIP, or
   ABSORB.

## Run Stats

| Metric | Value |
| --- | --- |
| Duration | 2h 6m, from 06:54 to 09:00 UTC |
| Threads spawned | 42 total: 27 completed, 14 timed out, 1 aborted |
| Query checkpoints | 20 |
| Tool calls | 13,757 |
| Document operations | 360 |
| Tokens | 4.9M input, 48K output, 505K cached |
| Cost | about $13.22 |
| Main agent turns | 55 |
| Final source size | 2,287 LOC across 5 Rust source files |
| Final test result | 252/252 passing, 100% |

The important signal is not just the final pass rate. The run reached full
correctness after multiple worker timeouts, retries, failed verification paths,
and a late semantic gap that required static resolution work. The adaptive loop
kept the project moving without cascading a missing phase into all downstream
work.

## Workqueue Evolution

The initial workqueue had 8 items:

| Item | Depends on |
| --- | --- |
| `scanner` | none |
| `ast` | none |
| `parser` | `scanner`, `ast` |
| `interpreter-core` | `parser` |
| `functions` | `interpreter-core` |
| `classes` | `functions` |
| `inheritance` | `classes` |
| `integration-test` | `inheritance` |

During the run, checkpoint decisions expanded the queue to 15 items:

| Item | Final status | Attempts | Notes |
| --- | --- | ---: | --- |
| `scanner` | done | 1 | Lexer and CLI wiring landed cleanly. |
| `ast` | done | 1 | AST types landed cleanly. |
| `parser` | done | 3 | Two timeouts followed by RETRY; succeeded on third attempt. |
| `interpreter-core` | split | 1 | Timed out and was split into focused interpreter sub-items. |
| `interpreter-runtime-foundation` | done | 1 | Values, environment, print, vars, blocks, basic expressions. |
| `interpreter-expressions` | done | 1 | Arithmetic, comparison, equality, unary, concatenation. |
| `interpreter-control-flow` | done | 2 | If/else, while, for desugaring, logical short-circuiting. |
| `functions` | done | 1 | Calls, arity, returns, closures, native clock. |
| `classes` | done | 1 | Classes, instances, fields, methods, `this`, constructors. |
| `inheritance` | done | 1 | Superclasses, overrides, `super`. |
| `integration-test` | exhausted | 3 | Reached 237/252; revealed missing resolver semantics. |
| `resolver-semantics` | mixed | 3 | Worker completed; verifier timed out. |
| `chapter-modes` | done | 1 | Scanning mode support for chapter-specific tests. |
| `resolver-static-errors` | done | 1 | Duplicate variable and parameter checks. |
| `resolver-lexical-binding` | done | 1 | Static lexical depth resolution. |

## Adaptive Split and Retry Behavior

The central orchestration win was the SPLIT decision on `interpreter-core`.
That item bundled too much: value representation, environment handling,
expression evaluation, statements, blocks, control flow, runtime errors, and
loop desugaring. It timed out after about 380 seconds without a usable merge.

The checkpoint evaluated the real project state and concluded that the work was
too broad for a single bounded thread. It split the item into:

- `interpreter-runtime-foundation`
- `interpreter-expressions`
- `interpreter-control-flow`

Each child was small enough to complete within the thread timeout. This is the
main value proposition of the adaptive supervised loop in
`coding-agent/prompts/orchestration/workflows/supervised.md`: failure is not
only a binary pass/fail signal, but a planning signal.

The parser phase showed the RETRY path. It timed out twice, but the checkpoint
kept the scope intact, raised context and timeout pressure, and let the third
attempt finish. That was the right call because the parser was large but still
cohesive; splitting it too early would have introduced boundary overhead without
clear independence.

The integration phase showed the limit of simple retry. After three attempts it
was exhausted at 237/252 tests. At that point the missing behavior was not a
small integration cleanup; it was a semantic resolver that had not been included
in the original workqueue. The main agent stepped out of the generated loop,
read failing tests directly, diagnosed the resolver requirement, spawned more
targeted resolver work, and then edited code directly to finish the tail.

## Worker, Verifier, and Merge Pattern

Each normal work item used a two-thread pattern:

1. A worker implemented the item in its own worktree.
2. A verifier inherited the worker's branch with `worktree_base`.
3. The verifier received the worker episode via `episodes=[worker_alias]`.
4. The verifier tested and fixed the worker's changes.
5. The supervisor merged only verified output.

This pattern separated generation from review while keeping both threads
isolated from the main branch until merge time. The adaptive loop also serialized
merges, because each merge mutates HEAD and can change the base for later work.

The pattern was effective for broad construction phases, but the final resolver
tail needed the main agent to synthesize failing test evidence across categories.
That suggests the worker/verifier pair is best for bounded implementation tasks;
semantic diagnosis still benefits from a central agent with full trace and file
context.

## Documents and Episodes

The run used both coordination channels available in tau's orchestration model:

- **Episodes** route prior thread traces into later threads.
- **Documents** provide mutable shared state for findings and verification notes.

The trace showed worker-to-verifier episode injection for normal phases:

```text
worker-scanner -> verifier-scanner
worker-parser -> verifier-parser
worker-interpreter-runtime-foundation -> verifier-interpreter-runtime-foundation
```

The resolver phase demonstrated multi-hop episode routing:

```text
worker-resolver-semantics -> verifier-resolver-semantics
worker-resolver-semantics,verifier-resolver-semantics -> worker-resolver-static-errors
worker-resolver-semantics,verifier-resolver-semantics -> worker-resolver-lexical-binding
worker-resolver-semantics,verifier-resolver-semantics,worker-resolver-lexical-binding -> verifier-resolver-lexical-binding
```

This matches the implementation model in `agent/src/orchestrator.rs`, where
completed episodes are retained by alias and formatted into a `# Prior episodes`
section for downstream threads.

Documents carried worker findings into verifier work. Examples from the trace
analysis:

```text
worker-scanner ==> scanner-findings (662 chars)
verifier-scanner <-- scanner-findings
verifier-scanner ==> scanner-verification-findings (930 chars)

worker-interpreter-runtime-foundation ==> interpreter-runtime-foundation (1457 chars)
verifier-interpreter-runtime-foundation <-- interpreter-runtime-foundation
verifier-interpreter-runtime-foundation ==> interpreter-runtime-foundation-verification (2066 chars)
```

The 360 document operations show that documents were not incidental logging.
They acted as a lightweight coordination bus for implementation notes,
verification findings, and readiness signals. `coding-agent/src/tools/document.rs`
emits `document_op` events for these reads and writes, which made the trace
auditable afterward.

## Issues Identified

### `_reference` path duplication in worktrees

The run used `worktree_include=["_reference"]` so worker worktrees could see
untracked test data. The issue analysis found that this copied the directory to
`_reference/_reference/` instead of copying only its contents. Threads worked
around the doubled path, but it wasted file searches and tool calls.

Future fix direction: make `worktree_include` semantics explicit and add a test
for directory-copy behavior in the worktree tool.

### Default timeout was too short for large phases

The default 300-second thread timeout was too short for `interpreter-core`, and
the phase timed out at about 380 seconds. The checkpoint recovered by splitting
the work, but a better initial scope estimate could avoid the expensive failed
attempt.

Future fix direction: derive timeout from item size, dependency depth, expected
test surface, and whether the item spans multiple language subsystems.

### Residual split children inherited bad timeout state

On a later split path, residual sub-items inherited very short remaining
timeouts, around 6 to 14 seconds. Those phantom timeouts produced exhausted
retries before the loop abandoned that branch.

Future fix direction: when splitting, reset child timeouts to a floor value
instead of inheriting a depleted parent timeout.

### Integration retry hid a missing planning item

`integration-test` exhausted three attempts at 237/252 because the original
workqueue omitted resolver semantics. Retrying integration could not invent that
missing architectural phase reliably.

Future fix direction: after repeated integration failures, checkpoint prompts
should explicitly ask whether the failure indicates a missing workqueue item,
not just a failed implementation attempt.

### Main-agent fallback was emergent

The main agent took over after the py_repl loop hit the long tail. It read the
15 failing test files, diagnosed static resolution, spawned targeted resolver
threads, and then edited `main.rs`, `parser.rs`, and `interpreter.rs` directly.

This fallback was valuable but not a first-class workflow. It should be captured
as a designed pattern: structured loop for bulk construction, central agent
diagnosis for semantic convergence, then targeted threads or direct edits.

## Follow-up Run: Self-Planned Workqueue

Issue comments also recorded a later self-planned run. Tau started from an empty
Lox project with only the reference tests and no pre-authored workqueue. It
first analyzed the tests, generated its own 8-item workqueue, and again reached
252/252 passing.

| Metric | Hand-authored run | Self-planned run |
| --- | ---: | ---: |
| Test result | 252/252 | 252/252 |
| Duration | 2h 6m | 71m 22s |
| Tool calls | 13,757 | 952 |
| Tokens | 4.9M input, 48K output | 2.29M input, 13.8K output |
| Cost | about $13.22 | about $5.92 |
| Final LOC | 2,287 across 5 files | 2,003 across 2 files |
| Workqueue size | 8 expanded to 15 | 8 expanded to 10 |
| Planning | pre-authored | self-generated |

The self-planned run suggests that up-front test-suite analysis can produce a
more natural dependency chain than a human-written queue. It also used a newer
model and chose a more monolithic implementation, so the improvement should not
be attributed to planning alone.

## Lessons for Future Orchestration Work

1. Treat checkpoint decisions as planning, not error handling. RETRY and SPLIT
   should encode why the previous shape was wrong.
2. Split by semantic boundary. `interpreter-core` became tractable when divided
   into runtime foundation, expression semantics, and control flow.
3. Keep dependency state durable on disk. The workqueue's attempts, statuses,
   and split lineage made the run resumable and auditable.
4. Use worker/verifier pairs for bounded construction, then let the main agent
   own cross-cutting diagnosis when failures span many subsystems.
5. Reset child-item budgets after SPLIT. A split child should receive enough
   time to be meaningful, regardless of the parent attempt's remaining budget.
6. Preserve trace-rich coordination. `episode_inject` and `document_op` events
   made it possible to reconstruct why a verifier knew what it knew.
7. Ask about missing phases after repeated integration failures. In this run,
   the resolver was not a small fix; it was a skipped architectural chapter.
8. Prefer self-planning for large greenfield builds when a rich test suite or
   reference corpus exists. The later self-planned run found a clean chapter-like
   decomposition before implementation began.

## Verification Checklist

For documentation maintenance, the repository-local paths referenced above
should exist:

```text
README.md
docs/design-orchestration.md
docs/trace-analysis.md
coding-agent/prompts/orchestration/overview.md
coding-agent/prompts/orchestration/workflows/supervised.md
coding-agent/prompts/orchestration/documents.md
coding-agent/src/tools/thread.rs
coding-agent/src/tools/document.rs
coding-agent/src/tools/query.rs
coding-agent/src/tools/py_repl.rs
coding-agent/src/tools/worktree.rs
agent/src/orchestrator.rs
tools/tau-trace/README.md
```
