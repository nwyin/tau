# Coordination Routing Benchmark

## Question

When the default agent is asked to choose an orchestration shape, does it
follow the requested topology, or does it escape into a different coordination
strategy?

This is the autonomy benchmark. It intentionally leaves orchestration choice
with the model and reports both:

- whether coordination succeeded
- whether the requested topology was actually followed

## Design

Two synthetic fixtures cover different dependency shapes:

- `fixtures/adversarial-routing/`
- `fixtures/dependent-review/`

Each fixture plants explicit anchors in the upstream artifacts and asks a
dependent critic/reviewer to synthesize them.

The benchmark prompt requests one of four orchestration variants, but unlike
`coordination-mechanism/`, the runner does not own the topology. This means
stronger models may self-correct into a better strategy than the one requested.

## Variants

1. `naive-parallel`
   - prompt requests a single parallel batch for all three threads
   - no episode injection
2. `prompt-only-parallel`
   - same requested launch shape
   - critic task text is strengthened to wait/read docs
3. `staged-pipeline`
   - prompt requests producers first, critic second with `episodes=[...]`
4. `document-polling`
   - prompt requests a parallel launch with doc polling before critic completion

## Scoring

Primary coordination metrics:

- `mechanism_success`
- `timing_success`
- `coordination_success`

Autonomy diagnostics:

- `requested_shape_followed`
- `variant_escape`
- `self_corrected_to_other_shape`

`variant_escape` makes prompt-following failures visible even when the model
still coordinates successfully by switching to another topology.

Secondary diagnostic:

- `synthesis_success`
  - scored from the final assistant answer
  - diagnostic only

Session/transport failures are reported separately from coordination success so
timeouts do not masquerade as pure routing failures.

## Outputs

`run.py` writes:

- generic reports: `report.md`, `report.json`
- coordination-specific reports: `coordination.md`, `coordination.json`

These include pass rates for:

- official benchmark success
- session success
- coordination success
- requested-shape fidelity
- variant escape/self-correction

## Usage

```bash
uv run python benchmarks/coordination-routing/run.py \
  benchmarks/coordination-routing/fixtures \
  --model gpt-5.4-mini \
  --runs 1 \
  -o benchmarks/coordination-routing/results
```
