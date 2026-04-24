# Self-Improving Harness Loop

## Context

Issue #8 asks whether tau can use an autoresearch-style loop to improve its
own harness. The target is not agent loop logic or task implementation. The
target is the model-facing harness surface: system prompt wording, tool
descriptions, tool schemas, and tool result formatting.

The loop should turn harness tuning into a repeatable experiment:

1. Capture a baseline score for the current checkout.
2. Let an agent make one targeted harness change.
3. Run the same eval matrix on the changed checkout.
4. Keep the change only when the composite score improves.
5. Repeat with a fresh hypothesis.

This document is the program file future agents should read before running
overnight experiments.

## Goals

- Make harness self-improvement executable, auditable, and reversible.
- Give the optimizing agent a single scalar metric.
- Keep eval prompts, fixtures, and scorecards fixed while prompt/tool-surface
  changes are explored.
- Prefer small, explainable hypotheses over random prompt churn.
- Produce run artifacts that can be compared across branches and commits.

## Non-Goals

- Do not replace the benchmark suite or invent subjective grading.
- Do not tune against one eval indefinitely.
- Do not mutate core tool behavior while evaluating model-facing harness text.
- Do not auto-commit changes without a human review step.

## Mutable Surface

The loop may modify:

- `coding-agent/src/system_prompt.rs`
- tool `description()` text
- tool `parameters()` schema text and field descriptions
- model-facing tool result formatting, including error text, truncation text,
  and recovery hints
- prompt fragments under `coding-agent/prompts/`

Every candidate change should be narrow enough to explain in one sentence.
Examples:

- "Make edit instructions emphasize exact old-string selection."
- "Shorten bash result framing so failure output is easier to scan."
- "Clarify when the agent should spawn threads versus continue locally."

## Fixed Surface

The loop must not modify:

- benchmark prompts, fixtures, or scorecards
- runner scoring code, except for explicit benchmark infrastructure work
- core tool implementations such as file writes, bash execution, or RPC flow
- agent loop control logic
- provider clients or model settings, unless the experiment is explicitly about
  provider behavior

If a future agent believes the fixed surface is wrong, it should stop and write
a proposal instead of folding that change into a harness-tuning experiment.

## Eval Matrix

Start with the cheapest useful matrix:

| Eval | Models | Runs |
|------|--------|------|
| `flask-books` | `gpt-5.4-mini`, `claude-haiku-4-5` | 1-3 |
| future fast online evals | same models | 1-3 |

Use 1 run while developing the loop. Use 2-3 runs before accepting a prompt or
tool-surface change, because model-in-loop evals are stochastic.

More evals are required before this loop should be trusted for autonomous
overnight optimization. With only `flask-books`, the expected failure mode is
overfitting the system prompt to a single greenfield Flask task.

## Score Contract

Each eval run must produce an objective pass ratio for one `(eval, model, run)`
cell:

```text
passed / total
```

The composite experiment score is:

```text
sum(passed / total for each eval x model x run cell)
```

The maximum score is the number of cells. The normalized score is:

```text
composite_score / max_score
```

Eval commands should emit one of the following:

```text
SELF_IMPROVE_EVAL {"passed": 5, "total": 6, "notes": "optional"}
```

or a benchmark report JSON at `{output_dir}/report.json` with
`summary.passed` and `summary.total`. The runner also accepts the legacy
`flask-books` scorecard line:

```text
Result: 5/6 passed
```

## Runner

Use `scripts/self-improve-experiment.py` to run a matrix and write a standard
report:

```bash
python3 scripts/self-improve-experiment.py \
  --label baseline \
  --eval flask-books=benchmarks/flask-books/run.sh \
  --model gpt-5.4-mini \
  --model claude-haiku-4-5 \
  --runs 1 \
  -o results/self-improve/baseline
```

For a Python benchmark that writes `report.json`, pass an eval command with an
`{output_dir}` placeholder:

```bash
python3 scripts/self-improve-experiment.py \
  --label experiment \
  --eval todo-tracking='uv run python benchmarks/todo-tracking/run.py benchmarks/todo-tracking/fixtures -o {output_dir}' \
  --model gpt-5.4-mini \
  -o results/self-improve/experiment
```

Then compare against a baseline:

```bash
python3 scripts/self-improve-experiment.py \
  --label experiment \
  --baseline results/self-improve/baseline/self-improve-report.json \
  --eval flask-books=benchmarks/flask-books/run.sh \
  --model gpt-5.4-mini \
  --runs 2 \
  -o results/self-improve/experiment
```

The script writes:

- `self-improve-report.json`
- one stdout log per eval/model/run
- one stderr log per eval/model/run

It prints a stable summary line:

```text
SELF_IMPROVE_SCORE composite=1.6667 max=2 normalized=0.8333
```

## Loop Protocol

### 1. Baseline

Before making a candidate change, run the matrix on a clean checkout:

```bash
git status --short
python3 scripts/self-improve-experiment.py ... --label baseline
```

Record the baseline report path. If the worktree is dirty, inspect it and do
not overwrite unrelated work.

### 2. Hypothesis

Write a short hypothesis before editing. Good hypotheses name a model-facing
mechanism and an expected eval effect.

Bad:

```text
Improve the prompt.
```

Good:

```text
Shorten the tool-use guideline block so small models spend fewer tokens before
starting the task. Expected effect: same pass rate, lower turns/tokens.
```

### 3. Candidate Change

Modify only the mutable surface. Keep diffs small. Do not edit evals.

Run cheap static checks for touched code:

```bash
cargo check -p coding-agent
python3 -m py_compile scripts/self-improve-experiment.py
```

### 4. Experiment

Run the same eval matrix with the same models and run count:

```bash
python3 scripts/self-improve-experiment.py ... \
  --label experiment \
  --baseline results/self-improve/baseline/self-improve-report.json
```

### 5. Decision

Keep the change when:

- composite score improves by more than the configured minimum delta
- no individual eval drops on repeated runs
- static checks still pass
- the diff is explainable and scoped

Discard the change when:

- composite score regresses
- gains are isolated to one eval while another eval drops
- the change adds task-specific wording from an eval prompt
- the model-facing text becomes longer without a measured benefit

### 6. Log

Append a short experiment note to the report directory or PR:

```text
Hypothesis: ...
Changed files: ...
Baseline: composite=...
Experiment: composite=...
Decision: keep/discard
Reason: ...
```

## Overfit Controls

- Rotate in new evals before trusting overnight runs.
- Keep a held-out eval set that the optimizing agent can run but not inspect.
- Reject task-specific prompt wording.
- Prefer improvements that hold across both a small OpenAI model and a small
  Anthropic model.
- Re-run winning changes with more seeds/runs before merge.
- Track token and wall-clock metrics as diagnostics, but do not let them
  override task success unless the pass score ties.

## Future Work

- Add more fast online evals so the search target is not `flask-books` only.
- Teach each benchmark runner to emit `SELF_IMPROVE_EVAL` directly.
- Add an optional "apply or revert" wrapper that snapshots candidate diffs.
- Persist experiment notes in a small SQLite or JSONL store for trend analysis.
- Add a held-out benchmark group that agents can execute but not edit.
