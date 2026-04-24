# Coordination Mechanism Benchmark

## Question

Given a runner-owned orchestration topology, does tau execute that scaffold
faithfully, and does the expected coordination mechanism actually work?

This benchmark is distinct from `coordination-routing/`: the runner owns the
topology here, so strong-model self-correction does not collapse the negative
controls.

## Design

The runner generates an exact variant-specific Python scaffold and forces the
main agent to execute it through a single top-level `py_repl` call.

Each fixture provides explicit metadata for:

- `pro_task`
- `con_task`
- `critic_task`
- `final_doc`

The scaffold:

1. launches the producers and critic according to the requested topology
2. lets the critic coordinate through episodes or shared docs, depending on the variant
3. writes a runner-owned final artifact to `final_doc`

Two fixtures are reused initially:

- `fixtures/adversarial-routing/`
- `fixtures/dependent-review/`

## Variants

1. `naive-parallel`
   - runner launches all three worker threads in one `tau.parallel(...)` batch
2. `prompt-only-parallel`
   - same runner-owned launch shape
   - critic task text explicitly requires waiting and document reads
3. `staged-pipeline`
   - runner launches upstream workers first
   - runner launches critic second with `episodes=[...]`
4. `document-polling`
   - runner still launches all three in one parallel batch
   - critic task text explicitly requires polling shared docs before completion

## Scoring

This benchmark hard-gates on:

- `scaffold_fidelity_success`
- `mechanism_success`
- `timing_success`

Primary fidelity checks:

- exactly one top-level `py_repl` tool call
- no other top-level tool calls
- normalized scaffold hash matches the runner-generated scaffold

Primary coordination checks:

- `staged-pipeline`: critic must receive both-source episode injection
- `document-polling`: critic must read both required docs after they are written
- `naive-parallel` / `prompt-only-parallel`: either coordination mechanism is acceptable

Secondary diagnostic:

- `synthesis_success`
  - scored from the content written to `final_doc`
  - diagnostic only, not a hard pass gate

Transport/session failures are reported separately from coordination success.

## Outputs

`run.py` writes:

- generic reports: `report.md`, `report.json`
- coordination-specific reports: `coordination.md`, `coordination.json`

These separate:

- official pass rate
- session success rate
- coordination success rate
- scaffold fidelity rate

## Usage

```bash
uv run python benchmarks/coordination-mechanism/run.py \
  benchmarks/coordination-mechanism/fixtures \
  --model gpt-5.4-mini \
  --runs 1 \
  -o benchmarks/coordination-mechanism/results
```
