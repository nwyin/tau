# Post-Edit Diagnostics: Edit-Cycle Reduction

Phase: 2 | Type: online | Cost: $2-5 per model | Time: ~45-120 minutes

## What it measures

Number of edit-test-fix cycles needed with and without immediate compiler/linter
diagnostics appended to the edit tool result.

## Why it matters for tau

opencode, oh-my-pi, and crush wire diagnostics into edit tool results so the
model sees type errors immediately, without a separate `bash` tool call for
`cargo check` or `tsc`. tau currently requires the model to explicitly run
compiler checks. The hypothesis: automatic diagnostics reduce wasted cycles
where the model moves on to the next step without realizing it introduced a
type error.

This is also the cheapest online benchmark — small task set, fast runs, clear
A/B signal.

## Prerequisites

- `shared/` infrastructure (TauSession, BenchConfig, Reporter)
- **Feature work**: ~150 LOC diagnostic hook in tau's FileEditTool:
  - Detect project type from file extension + config files
  - Shell out: `cargo check --message-format=json` / `tsc --noEmit` / `ruff check`
  - Parse structured output into `{file, line, message, severity}`
  - Append to edit tool result string

The benchmark can also run **without** the tau feature change by using a
system prompt variant: "After each edit, run `cargo check` and report errors."
This tests whether the benefit comes from automation or from the model simply
being told to check.

## Fixtures

Small refactoring tasks that commonly introduce type errors:

| Task | Language | What it tests |
|------|----------|--------------|
| Rename type parameter | Rust | Propagating type changes across modules |
| Update API signature | TypeScript | Call-site updates after signature change |
| Extract function (wrong return type) | Rust | Return type inference errors |
| Change struct field type | Rust | Field access type mismatches |
| Rename function + callers | Python | Import/reference updates |
| Add generic constraint | Rust | Constraint propagation |

4-6 tasks, each designed so a naive edit introduces 1-3 compiler errors
that need follow-up fixes.

### Fixture layout

```
fixtures/
├── rename-type-param/
│   ├── input/           # Code with type Foo<T>
│   ├── expected/        # Code with type Foo<U> (all sites updated)
│   └── prompt.md        # "Rename type parameter T to U in parser.rs"
├── update-api-sig/
│   ...
```

## Variants / run matrix

| Variant | Description | Implementation |
|---------|-------------|----------------|
| `no-diag` | Baseline: no compiler feedback | Standard tau |
| `prompt-check` | System prompt says "run cargo check after edits" | System prompt only |
| `auto-check` | Compiler output appended to edit result | tau feature (150 LOC) |
| `full-lsp` | LSP diagnostics on changed file | Future (if built) |

6 tasks x 4 variants x 3 runs = 72 runs.

## Procedure

```bash
# 1. Set up fixtures (manual creation — small task set)
# Fixtures committed to repo since they're hand-crafted

# 2. Run all variants
uv run python run.py fixtures/ \
    --model claude-sonnet-4-6 \
    --variants no-diag,prompt-check,auto-check \
    --runs 3 \
    -o results/

# 3. Single-variant debugging
uv run python run.py fixtures/ \
    --model claude-sonnet-4-6 \
    --variants auto-check \
    --filter "language=rust" \
    -o results/debug/
```

## Metrics

### Primary

- **Cycle count**: turns from "edit introduced error" to "error resolved".
  Lower is better. Measured by tracking edit tool calls and bash tool calls
  (for manual compiler checks).
- **Total turns per task**: end-to-end efficiency.
- **Task success rate**: did all changes compile and match expected output?

### Secondary

- **Diagnostic tokens**: extra tokens in edit result from diagnostic output.
  `auto-check` adds tokens but may save turns.
- **Self-correction rate**: when a diagnostic is shown, how often does the
  model fix it in the next turn? (Should be >90% for the benchmark to show
  value.)
- **Language breakdown**: Rust (strict types) vs TypeScript (gradual types)
  vs Python (dynamic). Hypothesis: Rust benefits most.

### Key comparison

```
Variant         Avg Cycles  Avg Turns  Success Rate  Token Cost
no-diag              3.2        12          67%         45K
prompt-check         2.1         9          83%         52K
auto-check           1.4         7          92%         48K
```

The delta between `prompt-check` and `auto-check` tells us whether the
benefit comes from automation or just awareness.

## Decision it informs

1. **Is post-edit compiler check sufficient, or is full LSP needed?**
   Hypothesis: compiler check captures 90% of the benefit at 0 complexity.

2. **Which languages benefit most?** Guides which language support to
   prioritize.

3. **Token tradeoff**: diagnostic output tokens vs saved retry turns.
   If `auto-check` uses fewer total tokens than `no-diag` despite the
   per-edit overhead, it's a clear win.

4. **Prompt vs feature**: if `prompt-check` matches `auto-check`, we can
   skip the feature work and just update the system prompt.

## Architecture

```
post-edit-diagnostics/
├── SPEC.md
├── run.py            # Runner with variant-specific tau configs
├── variants.py       # Variant definitions (system prompt overrides)
├── fixtures/         # Hand-crafted refactoring tasks (committed)
│   └── {task}/
│       ├── input/
│       ├── expected/
│       └── prompt.md
└── results/          # Output (gitignored)
```

Uses shared: `TauSession`, `BenchConfig`, `TaskResult`, `Reporter`, `Verifier`.

Estimated LOC: ~250 (run.py: ~150, variants.py: ~50, fixtures: ~50 lines of prompts)

### Special runner logic

The runner needs to count **cycles** — sequences of (edit -> error -> fix).
This requires parsing the conversation trace, not just final output.

Approach: after each task, read the tau trace log and count:
- `file_edit` tool calls
- `bash` tool calls containing compiler commands
- Consecutive (edit, error, edit) sequences

This cycle-counting logic is benchmark-specific, not shared.
