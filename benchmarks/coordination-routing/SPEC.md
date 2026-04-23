# Coordination Routing Benchmark

## Question

When a critic thread depends on upstream thread outputs, does tau actually
route that context into the critic, or does it synthesize only at the main
orchestrator layer after the critic already finished?

This benchmark is trace-first and targets the failure mode documented in
`notes/trace-analysis-6a118256.md`.

## Design

Single sharp synthetic fixture (`fixtures/adversarial-routing/`) with three
thread aliases:

- `position-for`
- `position-against`
- `critic`

Two upstream documents are required:

- `pro_case_notes`
- `con_case_notes`

Two anchor families are planted in task text:

- `PRO_*`
- `CON_*`

The final answer must include at least one anchor from each family.

## Variants

1. `naive-parallel`
   - Launch all three threads in one batch
   - No episode injection
2. `prompt-only-parallel`
   - Same launch shape as naive
   - Stronger critic instructions to wait/read docs
3. `staged-pipeline`
   - Launch `position-for` + `position-against`
   - Launch `critic` second with `episodes=[...]`
4. `document-polling`
   - Launch all three in parallel
   - Critic must poll/read docs and not complete early

## Scoring

Primary trace metrics (`score.py`):

- `episode_inject` into critic (and whether both upstream aliases are present)
- critic-side `document` reads for required docs
- critic read timing relative to upstream writes
- critic end timing relative to required artifact availability
- critic evidence citations

Secondary content metric:

- final answer contains at least one `PRO_*` marker and one `CON_*` marker
  - this is diagnostic only (reported), not a hard pass gate

Variant-aware expected mechanism:

- `staged-pipeline`: episode injection required
- `document-polling`: document reads-after-write required
- `naive-parallel` / `prompt-only-parallel`: either mechanism is acceptable

## Outputs

`run.py` writes:

- generic reports: `report.md`, `report.json`
- coordination-specific reports: `coordination.md`, `coordination.json`

These include per-variant pass rates and coordination-specific averages
(episode counts, document-read counts, marker retention, citation counts).

## Usage

```bash
uv run python benchmarks/coordination-routing/run.py \
  benchmarks/coordination-routing/fixtures \
  --model gpt-5.4-mini \
  --runs 1 \
  -o benchmarks/coordination-routing/results
```
