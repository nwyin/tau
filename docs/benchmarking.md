# Benchmarking Strategy

tau's benchmarking answers two distinct questions:

1. **How good is the harness?** System-level: startup latency, memory footprint, token overhead, wall-clock time.
2. **How good is the model in this harness?** Capability-level: can the model complete real tasks end-to-end?

Both matter. A fast harness that produces bad completions is useless. A great model hamstrung by a bloated system prompt is leaving performance on the table.

## The native harness hypothesis

Models may perform differently depending on the harness driving them. A "native" harness — the one built by the model provider — has advantages: tightly tuned system prompts, tool schemas the model was fine-tuned against, and steering logic informed by internal evals. The question is: **how much does that advantage matter, and does it vary by model size?**

The test matrix:

| Model | Native Harness | tau | Delta |
|-------|---------------|-----|-------|
| claude-sonnet-4-6 | Claude Code | tau | ? |
| claude-haiku-4-5 | Claude Code | tau | ? |
| gpt-5.4 | Codex CLI | tau | ? |
| gpt-5.4-nano | Codex CLI | tau | ? |

Hypotheses to validate:
- **Frontier models are harness-agnostic.** Large models (Opus, GPT-5.4) should perform similarly regardless of harness because they're robust to prompt variation.
- **Small models are harness-sensitive.** Nano/Haiku-class models are more affected by system prompt quality, tool schema design, and steering logic. A well-tuned harness matters more here.
- **Native harnesses have an unfair advantage on tool schema.** Models may be RLHF'd against specific tool formats. If Claude Code uses a particular edit tool schema, Claude may perform worse with a different schema even if both are semantically equivalent.

If tau matches or beats native harnesses on small models, the harness design is validated. If there's a gap, diffing system prompts and tool schemas against the native harness reveals what to improve.

## Cross-harness comparison

Beyond native harnesses, compare tau against other open-source harnesses running the same model:

| Harness | Language | Interesting because |
|---------|----------|-------------------|
| [pi-mono](https://github.com/anthropics/pi-mono) | TypeScript | tau's direct ancestor; measures what the port lost or gained |
| [opencode](https://github.com/nichochar/opencode) | Go | Similar scope (minimal coding agent), different language |
| [aider](https://github.com/paul-gauthier/aider) | Python | Mature, well-benchmarked, strong baseline |
| [Codex CLI](https://github.com/openai/codex) | TypeScript | OpenAI's native harness |
| Claude Code | N/A (closed) | Anthropic's native harness |

Dimensions to compare:

- **Task pass rate** — same prompt, same model, which harness completes the task?
- **Token efficiency** — tokens consumed for equivalent tasks (measures system prompt + tool overhead)
- **Wall-clock time** — end-to-end including startup, streaming, tool execution
- **Startup latency** — time from invocation to first API call (Rust binary vs Node/Go/Python runtime)
- **Memory footprint** — peak RSS during a standard task
- **Cost** — total API spend per task (function of token efficiency)

## Eval tasks

Evals live in `benchmarks/`. Each eval is a directory with:
- `prompt.txt` — the task prompt (portable across harnesses)
- `run.sh` — runs the eval against tau, validates output, prints a scorecard
- `README.md` — what the eval measures and why (optional)

### Current evals

**flask-books** — Create a Flask app with SQLite, templates, JSON API, and tests. Exercises file write, bash (pip install, pytest), multi-step planning. 6-point scorecard, pass/fail on `pytest` exit code. Expected: 3-6 turns for a capable model.

### Planned evals

**refactor-extract** — Given a single large file, extract a function into a new module, update imports, run existing tests. Exercises file read, edit, bash. Tests that the agent can work with existing code, not just greenfield.

**debug-failing-test** — Given a repo with a failing test and a subtle bug, fix the bug. Exercises error reading, hypothesis formation, targeted edits. The edit tool is critical here.

**multi-file-feature** — Add a feature that spans 3+ files (e.g., add a new API endpoint with model, route, and test). Exercises planning and coordination across files.

### Designing good evals

A good eval for harness comparison:
- **Has an objective pass/fail criterion.** Tests pass, output matches, file exists. No subjective grading.
- **Requires multiple tools.** Single-tool tasks don't stress the agent loop.
- **Has a natural error recovery path.** If the first attempt fails (tests don't pass), a good harness helps the model recover. Evals that are one-shot-or-nothing don't measure the agent loop value.
- **Is model-portable.** The prompt should work with any model, not rely on provider-specific features.
- **Completes in bounded turns.** If a capable model can't finish in ~8 turns, the task is too large or underspecified.

## System performance benchmarks

Separate from capability evals, measure the harness itself:

- **SSE parsing throughput** — bytes/sec through the event parser (criterion bench on fixtures)
- **Message serialization** — round-trip time for realistic conversation histories
- **Startup to first API call** — cold start latency of the binary
- **Memory under load** — RSS with 100-message conversation history

These don't need an API key and can run in CI.

## Running benchmarks

```bash
# Capability eval with a specific model
OPENAI_API_KEY=sk-... ./benchmarks/flask-books/run.sh --model gpt-5.4-nano

# Same eval, different model
ANTHROPIC_API_KEY=sk-... ./benchmarks/flask-books/run.sh --model claude-haiku-4-5

# System perf (no API key needed, once criterion benches exist)
cargo bench
```

---

## Release-gated tracking

Track tau's quality over time with a model x harness-version matrix:

```
                    v0.1.0    v0.2.0    v0.3.0    codex     claude-code
gpt-5.4-mini          —        62%       68%       71%         —
claude-sonnet-4.6      —        71%       75%        —         82%
gemini-3.1-pro         —        58%        —         —          —
devstral-2512          —        45%       52%        —          —
```

Three axes: across releases (did harness changes improve scores?), across models (which models work best with tau?), and vs other harnesses (how does tau compare?).

### When to run

**On release tags only.** Benchmarks are expensive (~$20-100 per matrix run), so they gate on `v*` tags, not every commit. For local iteration, use edit-bench (free, fast, local).

### What to run

**Terminal-bench-core** — 71 Docker-based tasks. For cost control, run a **stable subset of ~30 tasks** selected for category coverage, reasonable solve times, and discriminating power. Subset defined in `benchmarks/task-subset.txt`.

Model matrix per release:

| Model | Provider | Why |
|-------|----------|-----|
| `gpt-5.4-mini` | OpenAI (Responses) | Cheap, fast baseline |
| `claude-sonnet-4.6` | Anthropic (Messages) | Strong, different provider |
| `google/gemini-3.1-flash-lite-preview` | OpenRouter (Chat) | Tests the openai-chat backend |

### Infrastructure

The Harbor adapter (`benchmarks/harbor/tau_agent.py`) handles binary upload, API key forwarding, stats collection, and model selection. CI invocation:

```bash
harbor run \
  --agent benchmarks.harbor.tau_agent:TauAgent \
  --dataset terminal-bench-core@0.1.1 \
  --tasks-file benchmarks/task-subset.txt \
  --model $MODEL \
  --output benchmarks/results/${VERSION}_${MODEL_SAFE}.json
```

### Results format

Each run produces `benchmarks/results/{version}_{model}.json`:

```json
{
  "version": "v0.2.0",
  "model": "gpt-5.4-mini",
  "summary": {
    "total_tasks": 30,
    "resolved": 19,
    "accuracy": 0.633,
    "total_cost_usd": 4.25
  },
  "tasks": [{ "task_id": "...", "resolved": true, "input_tokens": 45000, "wall_clock_sec": 85 }]
}
```

An aggregation script (`benchmarks/aggregate.py`, TBD) reads all result JSONs and produces: matrix table, trend chart data, cost report, and regression detection (flag >5% accuracy drops).

### Cost estimate

Per release, 30 tasks x 3 models: ~$18 API + ~$5-10 Harbor compute = **~$25-30 per release**.

### Implementation order

1. Define task subset — curate `benchmarks/task-subset.txt`
2. Write `aggregate.py` — reads result JSONs, produces `SUMMARY.md`
3. Add CI job — benchmark workflow triggered on `v*` tags
4. First release run — tag, validate end-to-end
5. Manual baseline — record codex/claude-code scores for comparison columns
