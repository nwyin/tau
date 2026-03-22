# Benchmark Tracking: Release-Gated Terminal-Bench Evaluation

## Goal

Track tau's quality over time with a model × harness-version matrix:

```
                    v0.1.0    v0.2.0    v0.3.0    codex     claude-code
gpt-5.4-mini          —        62%       68%       71%         —
claude-sonnet-4.6      —        71%       75%        —         82%
gemini-3.1-pro         —        58%        —         —          —
devstral-2512          —        45%       52%        —          —
```

Two axes of comparison:
1. **Across releases** (columns) — did harness changes improve scores?
2. **Across models** (rows) — which models work best with tau's tool format?
3. **vs other harnesses** (rightmost columns) — how does tau compare?

## When to Run

**On release tags only.** Benchmarks are expensive (~$20-100 per matrix run), so
they gate on `v*` tags, not every commit.

```
git tag v0.2.0 && git push origin v0.2.0
# → CI builds musl binary
# → CI triggers benchmark workflow
# → Harbor runs terminal-bench-core subset
# → Results committed to benchmarks/results/
```

For local iteration, use edit-bench (free, fast, local).

## What to Run

**Terminal-bench-core** is the canonical benchmark — 71 Docker-based tasks spanning
file manipulation, git operations, debugging, scripting, and data processing.

For cost control, we run a **stable subset of ~30 tasks** selected for:
- Coverage across categories (not all file-manipulation tasks)
- Reasonable solve times (skip tasks that routinely timeout)
- Discriminating power (skip tasks all harnesses ace or all fail)

The subset is defined in `benchmarks/task-subset.txt` — one task ID per line.
This file changes rarely and only with justification.

### Model Matrix

Each release runs against a **fixed set of models** that covers the provider
spectrum:

| Model | Provider | Why |
|-------|----------|-----|
| `gpt-5.4-mini` | OpenAI (Responses) | Cheap, fast baseline |
| `claude-sonnet-4.6` | Anthropic (Messages) | Strong, different provider |
| `google/gemini-3.1-flash-lite-preview` | OpenRouter (Chat) | Tests the openai-chat backend |

Adding models to the matrix is a conscious decision (each adds ~$5-20 per release).

## How to Run

### Infrastructure

```
GitHub Actions (CI)
  │
  ├─ build musl binary (existing job)
  │
  └─ benchmark job (new, on v* tags only)
       │
       ├─ download musl binary from build job
       ├─ invoke Harbor CLI with task subset + model matrix
       │     Harbor provisions cloud containers (Daytona/Modal)
       │     uploads tau binary, runs tasks, collects results
       └─ commit results JSON to benchmarks/results/
```

### Harbor Invocation

The existing `benchmarks/harbor/tau_agent.py` adapter handles:
- Binary upload to container
- API key forwarding
- Stats JSON collection (tokens, cost)
- Model selection via `TAU_MODEL`

The CI job invokes Harbor's CLI:

```bash
harbor run \
  --agent benchmarks.harbor.tau_agent:TauAgent \
  --dataset terminal-bench-core@0.1.1 \
  --tasks-file benchmarks/task-subset.txt \
  --model $MODEL \
  --env TAU_BINARY_PATH=dist/tau-x86_64-unknown-linux-musl \
  --output benchmarks/results/${VERSION}_${MODEL_SAFE}.json
```

### Required Secrets

GitHub Actions secrets for the benchmark job:
- `OPENAI_API_KEY` — for OpenAI models
- `ANTHROPIC_API_KEY` — for Anthropic models
- `OPENROUTER_API_KEY` — for OpenRouter models
- `HARBOR_API_KEY` — for Harbor cloud provisioning (Daytona/Modal credentials)

## Results Format

Each run produces a JSON file at `benchmarks/results/{version}_{model}.json`:

```json
{
  "version": "v0.2.0",
  "commit": "abc1234",
  "model": "gpt-5.4-mini",
  "provider": "openai",
  "api": "openai-responses",
  "timestamp": "2026-03-22T18:00:00Z",
  "task_subset": "task-subset-v1",
  "summary": {
    "total_tasks": 30,
    "resolved": 19,
    "accuracy": 0.633,
    "total_input_tokens": 1250000,
    "total_output_tokens": 85000,
    "total_cost_usd": 4.25,
    "total_wall_clock_sec": 2400
  },
  "tasks": [
    {
      "task_id": "create-python-script-001",
      "resolved": true,
      "input_tokens": 45000,
      "output_tokens": 3200,
      "wall_clock_sec": 85,
      "error": null
    }
  ]
}
```

## Results Aggregation

A script (`benchmarks/aggregate.py`) reads all JSON files in `benchmarks/results/`
and produces:

1. **Matrix table** (markdown) — the version × model grid shown above
2. **Trend chart data** (JSON) — accuracy over time per model
3. **Cost report** — total spend per release, per model
4. **Regression detection** — flag if accuracy drops >5% from previous release

Output: `benchmarks/results/SUMMARY.md` — committed alongside results.

## Workflow: CI Job

```yaml
  benchmark:
    name: Benchmark (Terminal-Bench)
    runs-on: ubuntu-latest
    needs: [build-musl]
    if: startsWith(github.ref, 'refs/tags/v')
    strategy:
      matrix:
        model:
          - gpt-5.4-mini
          - claude-sonnet-4.6
          - google/gemini-3.1-flash-lite-preview
    steps:
      - uses: actions/checkout@v4

      - uses: actions/download-artifact@v4
        with:
          name: tau-x86_64-linux-musl
          path: dist/

      - uses: actions/setup-python@v5
        with:
          python-version: "3.12"

      - name: Install Harbor + dependencies
        run: pip install harbor-bench terminal-bench

      - name: Run benchmark
        env:
          OPENAI_API_KEY: ${{ secrets.OPENAI_API_KEY }}
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
          OPENROUTER_API_KEY: ${{ secrets.OPENROUTER_API_KEY }}
          TAU_BINARY_PATH: dist/tau-x86_64-unknown-linux-musl
        run: |
          VERSION=${GITHUB_REF#refs/tags/}
          MODEL_SAFE=$(echo "${{ matrix.model }}" | tr '/' '_')
          harbor run \
            --agent benchmarks.harbor.tau_agent:TauAgent \
            --dataset terminal-bench-core@0.1.1 \
            --tasks-file benchmarks/task-subset.txt \
            --model "${{ matrix.model }}" \
            --output "benchmarks/results/${VERSION}_${MODEL_SAFE}.json"

      - uses: actions/upload-artifact@v4
        with:
          name: bench-${{ matrix.model }}
          path: benchmarks/results/

      # Aggregate + commit results after all matrix jobs complete
      # (handled by a separate post-benchmark job)
```

## Harness Adapter Status

The rename-followup adapter work is already done in-tree:

1. `install-tau.sh.j2` and `tau_agent.py` now use the `tau` binary name.
2. Harbor forwards `OPENROUTER_API_KEY` alongside the existing provider keys.
3. The Harbor adapter reports version from `TAU_VERSION`.

The remaining work is CI orchestration, task subset definition, result aggregation, and release tagging discipline.

## Comparison with Other Harnesses

For the "vs other harnesses" columns, we pull published scores:
- **Codex CLI**: from terminal-bench leaderboard (if published)
- **Claude Code**: from terminal-bench leaderboard
- **Aider**: from aider's published polyglot benchmarks (different benchmark,
  but directionally useful)

These are manually updated in the summary since they don't run in our CI.

## Cost Estimate

Per release, running 30 tasks × 3 models:

| Model | Est. cost/task | 30 tasks |
|-------|---------------|----------|
| gpt-5.4-mini | ~$0.15 | ~$4.50 |
| claude-sonnet-4.6 | ~$0.40 | ~$12.00 |
| gemini-3.1-flash-lite | ~$0.05 | ~$1.50 |
| **Total per release** | | **~$18** |

Plus Harbor compute costs (~$5-10 for container provisioning).
Total: **~$25-30 per release** — manageable for monthly releases.

## Implementation Order

1. **Define task subset** — curate `benchmarks/task-subset.txt` from terminal-bench-core
2. **Fix Harbor adapter** — binary name, key forwarding, version reporting
3. **Write aggregate.py** — reads result JSONs, produces SUMMARY.md
4. **Add CI job** — benchmark workflow triggered on `v*` tags
5. **First release run** — tag v0.2.0, validate end-to-end
6. **Manual baseline** — record codex/claude-code scores for comparison columns
