# Harbor gpt-5.4-nano Aider Polyglot Results

Source: GitHub issue #10, compared against GitHub issue #9.

This run evaluated `gpt-5.4-nano` through tau's Harbor adapter on the
`aider-polyglot@1.0` Python subset. Harbor's binary reward is strict: a task
scores 1.0 only when all tests pass. By that metric the run was `0/10`, but
the individual test breakdown shows materially better first-pass code than the
earlier `gpt-4.1-nano` run in issue #9.

## Setup

| Field | Value |
|-------|-------|
| Model | `gpt-5.4-nano` reasoning model via Codex OAuth |
| Dataset | `aider-polyglot@1.0`, Python tasks only (`polyglot_python_*`) |
| Tasks | 10 |
| Concurrency | 1 |
| Runtime | Local Docker via OrbStack |
| Binary | Linux arm64 binary uploaded through `environment.upload_file()` |

Relevant repo entry points:

- Harbor strategy and result format: `docs/benchmarking.md`
- Harbor binary install/upload behavior: `docs/releases.md`
- Harbor tau adapter: `benchmarks/harbor/tau_agent.py`

## Headline Results

| Metric | Result |
|--------|--------|
| Binary pass rate | 0/10 tasks, 0% |
| Individual tests passed | 64/211, 30% |
| Total cost | $0.028 |
| Average cost/task | $0.0028 |
| Average duration/task | 12.1s |
| Average tool calls/task | 4.4 |

The strict binary score hides near-misses. Two tasks reached at least 90%
individual test pass rate:

- `variable-length-quantity`: 25/26 tests passed, 96%
- `list-ops`: 22/24 tests passed, 92%

Those are the tasks most likely to flip from 0.0 to 1.0 if the harness can
push the model through a test-read-fix loop instead of accepting the first
implementation.

## Per-Task Breakdown

| Task | Tests Passed | Pass Rate | Tools | Tokens In/Out | Cost | Duration |
|------|--------------|-----------|-------|---------------|------|----------|
| `variable-length-quantity` | 25/26 | 96% | 3 | 4,605 / 721 | $0.0019 | 6.1s |
| `list-ops` | 22/24 | 92% | 2 | 3,971 / 666 | $0.0016 | 7.2s |
| `hangman` | 4/7 | 57% | 5 | 8,661 / 798 | $0.0028 | 9.9s |
| `sgf-parsing` | 5/23 | 22% | 7 | 12,272 / 4,428 | $0.0085 | 28.4s |
| `two-bucket` | 2/9 | 22% | 3 | 5,053 / 1,579 | $0.0030 | 12.4s |
| `react` | 2/14 | 14% | 6 | 7,793 / 1,360 | $0.0033 | 15.8s |
| `grep` | 3/25 | 12% | 3 | 4,928 / 747 | $0.0019 | 6.5s |
| `phone-number` | 1/21 | 5% | 3 | 5,369 / 718 | $0.0020 | 6.3s |
| `beer-song` | 0/8 | 0% | 5 | 6,151 / 1,111 | $0.0030 | 13.6s |
| `forth` | 0/54 | 0% | 7 | 8,259 / 1,560 | $0.0037 | 15.2s |

## Comparison With Issue #9

Issue #9 ran `gpt-4.1-nano` on 5 `aider-polyglot` Python tasks. Issue #10
expanded the sample to 10 tasks with `gpt-5.4-nano`, including the same 5
tasks from issue #9.

| Metric | issue #9: `gpt-4.1-nano` | issue #10: `gpt-5.4-nano` |
|--------|--------------------------|---------------------------|
| Binary pass rate | 0/5, 0% | 0/10, 0% |
| Individual tests passed | 25/80, 31% | 64/211, 30% |
| Average tool calls/task | 4.6 | 4.4 |
| Average cost/task | $0.0009 | $0.0028 |
| Average duration/task | 9.1s | 12.1s |
| Used bash? | No | Yes, on `beer-song` |
| Used glob? | No | Yes, on most tasks |

On aggregate partial pass rate, the two runs look nearly identical: 31% versus
30%. The distribution is different, though. `gpt-5.4-nano` produced more
top-heavy results, with two tasks above 90%, while `gpt-4.1-nano` had one such
near-miss.

### Overlapping Tasks

| Task | `gpt-4.1-nano` | `gpt-5.4-nano` | Delta |
|------|----------------|----------------|-------|
| `list-ops` | 22/24, 92% | 22/24, 92% | 0 points |
| `two-bucket` | 1/9, 11% | 2/9, 22% | +11 points |
| `react` | 2/14, 14% | 2/14, 14% | 0 points |
| `grep` | 0/25, 0% | 3/25, 12% | +12 points |
| `beer-song` | 0/8, 0% | 0/8, 0% | 0 points |
| **Total** | **25/80, 31%** | **29/80, 36%** | **+5 points** |

The overlapping-task comparison shows only a modest improvement. The stronger
signal is not broad uplift across the shared set; it is that `gpt-5.4-nano`
can get very close to correct on some tasks but still needs a validation loop
to harvest that extra correctness.

## Failure Pattern

The dominant failure mode is not a complete inability to write code. It is
first-pass code with unobserved edge-case bugs:

- `variable-length-quantity`: 25/26 tests passed, likely one missed edge case.
- `list-ops`: 22/24 tests passed in both runs, suggesting a stable near-miss
  that test feedback should expose quickly.
- `beer-song`: failed with the same broad shape as issue #9, returning the
  wrong type rather than converging on the Exercism contract.
- `react`: remained mostly incomplete despite the stronger model.

The key harness gap is that the model does not naturally run the task tests
after editing. In issue #10 it used `bash` once for exploration, but still did
not turn available shell access into test-driven iteration.

## Takeaways For Test Iteration

1. Add an explicit test iteration path before treating model choice as the main
   lever. The 90%+ near-misses are the cheapest likely source of binary pass
   gains.
2. Measure a forced validation variant on the same task subset:
   - after editing, run the task's local test command;
   - feed failures back into the model;
   - allow one or two repair turns;
   - compare binary pass rate, partial pass rate, cost, and duration.
3. Keep binary reward, but always report partial tests for small local sweeps.
   A `0/10` headline is true but not diagnostic enough for harness work.
4. Prefer a small `RunTestsTool` or benchmark scaffold guidance over a broad
   prompt rewrite. The missing behavior is concrete: run tests, read failures,
   patch the edge case, repeat within a bounded budget.
5. Start with `variable-length-quantity`, `list-ops`, and `hangman` as the
   smoke set for test iteration. They have enough correct code to show whether
   the loop converts near-misses into Harbor binary passes.

## Recommended Next Experiment

Run the same `aider-polyglot` Python subset with a test-iteration variant:

| Variant | Max repair turns | Expected signal |
|---------|------------------|-----------------|
| Baseline | 0 | Reproduce the issue #10 first-pass behavior |
| Prompt guidance | 1 | Tests whether instruction alone causes bash test usage |
| Dedicated test tool | 1-2 | Tests whether an explicit affordance improves pass rate |

Success criterion: at least one of the 90%+ tasks flips to binary pass without
erasing the cost advantage. At issue #10 prices, even a 2x-3x increase in cost
would still be cheap enough for local benchmark iteration.
