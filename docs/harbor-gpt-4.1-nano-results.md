# Harbor gpt-4.1-nano First Run Results

Source: GitHub issue #9, "Harbor benchmark: first run results with gpt-4.1-nano"
([nwyin/tau#9](https://github.com/nwyin/tau/issues/9)), opened 2026-03-20.

This records the first end-to-end Harbor benchmark run for tau using OpenAI
`gpt-4.1-nano` through the Codex OAuth billing path. The result is primarily a
pipeline validation: Harbor could run tau, upload a Linux binary into the task
container, authenticate through `~/.codex/auth.json`, collect stats, and report
task-level cost and token usage.

## Setup

| Field | Value |
|-------|-------|
| Model | `gpt-4.1-nano` |
| Auth path | Codex OAuth via `~/.codex/auth.json` |
| Infra | Local Docker through OrbStack on Apple Silicon (`arm64`) |
| Binary | Linux `arm64` release binary cross-compiled with `docker run rust:1.87-bookworm cargo build --release` |
| Binary delivery | Direct Harbor upload through `environment.upload_file()` |
| Relevant adapter | `benchmarks/harbor/tau_agent.py` |
| Direct upload commit | `46ee3790bbb4853ff0083486e96832bcbe1f8328` |

## Harbor, Docker, and Codex OAuth Notes

The Harbor adapter is the repo integration point for this run. It locates a
locally built tau binary, uploads it into the Harbor task environment, and falls
back to the install script when no local binary is available.

For OpenAI-family models, tau can use Codex OAuth instead of `OPENAI_API_KEY`.
During this benchmark, the adapter read host `~/.codex/auth.json`, forwarded it
as `CODEX_AUTH_JSON`, and recreated the file inside the Harbor task container
before launching tau. That validated the Codex OAuth path under Harbor without
requiring an OpenAI API key.

The local Docker path was good enough for small, fast tasks, but it was not
stable for heavy benchmark environment builds. FeatureBench-lite failed before
agent execution because OrbStack crashed or ran out of disk while building large
containers. Treat local Docker as suitable for smoke tests and lightweight
subsets, not as the target infrastructure for heavy benchmark suites.

## Results

### hello-world: 1/1 binary pass

| Metric | Value |
|--------|-------|
| Reward | 1.0 |
| Duration | 3.7s |
| Tool calls | 1 (`file_write`) |
| Tokens | 1,427 in / 58 out |
| Cost | $0.00017 |

### aider-polyglot Python subset: 0/5 binary pass

Harbor reported binary reward: a task scores 0 when any test fails.

| Task | Tools | Tokens in/out | Cost | Duration | Failure mode |
|------|-------|---------------|------|----------|--------------|
| beer-song | 3 | 5,323 / 323 | $0.00086 | 4.6s | Wrong return type (`str` vs `list`) |
| grep | 3 | 3,786 / 561 | $0.00063 | 7.1s | Logic errors in flag handling |
| list-ops | 3 | 4,048 / 470 | $0.00059 | 7.3s | Incorrect implementation |
| react | 7 | 7,889 / 839 | $0.00116 | 12.7s | Edit errors and wrong reactive logic |
| two-bucket | 7 | 6,190 / 1,160 | $0.00118 | 13.8s | Algorithm errors |

### Partial test results

The individual tests show more signal than the binary pass rate. In particular,
`list-ops` was close enough that a test-run-and-repair loop likely would have
finished it.

| Task | Passed | Failed | Total | Pass rate | Notes |
|------|--------|--------|-------|-----------|-------|
| `list-ops` | 22 | 2 | 24 | 92% | Only `foldr` wrong; accumulator direction was reversed |
| `react` | 2 | 12 | 14 | 14% | Skeleton barely filled in; `value` returned `None` |
| `two-bucket` | 1 | 8 | 9 | 11% | Wrong algorithm and wrong return format |
| `beer-song` | 0 | 8 | 8 | 0% | Returned a string instead of a list and missed pluralization |
| `grep` | 0 | 25 | 25 | 0% | Total failure |
| Total | 25 | 55 | 80 | 31% | Binary reward hid partial progress |

### featurebench-lite: 0/5 ran

No featurebench-lite tasks reached agent execution. All failed during Docker
environment build. The local OrbStack setup crashed under the heavier container
builds for packages and projects such as mlflow, seaborn, metaflow, xarray, and
pydantic, then eventually ran out of disk.

## Observations

1. The Harbor pipeline works. Binary upload, Codex OAuth, stats collection, and
   Harbor integration all completed on the small tasks.
2. `gpt-4.1-nano` is too weak for reliable code generation in this harness. It
   produced plausible code, but it did not pass any aider-polyglot Python subset
   task at binary reward granularity.
3. Partial progress matters. The 31% individual test pass rate, including 92%
   on `list-ops`, is useful harness feedback that binary task reward hides.
4. The agent did not self-test. `BashTool` was available, but the model did not
   run pytest before finishing. This supports adding stronger test-running
   guidance or a dedicated `RunTestsTool`.
5. Heavy benchmark suites need cloud infrastructure. FeatureBench and SWE-bench
   style workloads should run on Modal or a VM with enough disk, not local
   laptop Docker.
6. Nano-class runs are very cheap. The observed cost was about $0.001 per task;
   a full 225-task aider-polyglot run was estimated around $0.25.

## Follow-ups

- [ ] Run the same Harbor path with `gpt-4.1-mini` or `o4-mini` to get meaningful
      pass rates.
- [ ] Add test-running guidance to the system prompt or implement a dedicated
      `RunTestsTool`.
- [ ] Move heavy benchmarks such as FeatureBench and SWE-bench to Modal or an
      adequately provisioned VM.
- [ ] Keep the aider-polyglot Python subset as a lightweight local benchmark:
      containers build quickly and tasks run in seconds.
- [ ] Try Anthropic models after an API key is available, then compare against
      the Codex OAuth OpenAI-family path.
