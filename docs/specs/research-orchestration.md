# Research Orchestration: Long-Running Multi-Thread Experiment Management

**Status**: Draft  
**Date**: 2026-04-05  
**Scope**: Tau harness modifications + experiment infrastructure contract for autonomous ML research

---

## Problem

Tau's orchestration layer was built for software engineering tasks: spawn threads, do
focused 5-minute work, return results. ML research has fundamentally different
requirements:

- **Hours, not minutes.** A single GPU training run takes 1-8 hours. A scaling grid is
  12-36 runs across days.
- **External compute.** Threads don't run the GPU work themselves -- they submit jobs to
  Modal, poll for completion, fetch results. The thread is a project manager, not a
  laborer.
- **Intermediate communication.** Thread A discovers that BPE tokenization needs vocab
  size 8192 to work well. Thread B needs that finding *now*, not after A finishes.
- **Multi-session persistence.** A scaling study spans a week. Session restarts must not
  lose accumulated results.
- **Adaptive planning.** After seeing initial results, the orchestrator should reallocate
  compute: skip a saturated axis, double down on a promising one.

The current system gets close. The supervised loop pattern (py_repl + workqueue JSON +
checkpoint/replan) handles phased execution and adaptation. Documents provide inter-thread
state sharing. Worktrees isolate file edits. But five specific gaps block research
workflows.

---

## Current State: What Works

| Capability | Mechanism | Reference |
|---|---|---|
| Thread spawning | In-process tokio tasks with alias reuse | `thread.rs:147-573` |
| Parallel execution | `tau.parallel(*specs)` in py_repl | `py_repl.rs:422-477` |
| Inter-thread data | Virtual documents (read/write/append) | `document.rs:121-203`, `orchestrator.rs:199-236` |
| Worktree isolation | Git branches per thread, auto-commit | `worktree.rs`, `thread.rs:257-285` |
| Episode injection | Compact traces injected as system prompt context | `orchestrator.rs:239-252` |
| Checkpoint/replan | Supervised loop with workqueue JSON | `prompts/orchestration/workflows/supervised.md` |
| Structured outcomes | complete/abort/escalate with evidence | `thread.rs:173`, `agent/src/thread.rs:8-48` |
| Concurrency control | Semaphore-based, default 10 threads | `orchestrator.rs:87-96` |

---

## Five Gaps

### Gap 1: Hard-coded 25-turn limit

A thread monitoring a 4-hour Modal job needs to: submit, poll status every few minutes,
fetch results, analyze, maybe resubmit. That is easily 50-100 turns.

```rust
// thread.rs:374 -- currently hard-coded
max_turns: Some(25),
```

No parameter exists to override this per-thread. Research threads hit the turn limit
long before their timeout.

### Gap 2: Volatile documents

Documents live in `OrchestratorState` as an in-memory `RwLock<HashMap<String, String>>`
(`orchestrator.rs:68`). Session restart erases everything. A week-long scaling study
accumulates findings, partial results, and cross-thread coordination state that must
survive restarts.

### Gap 3: No fire-and-forget threads

`tau.thread()` in py_repl blocks until the thread completes (`py_repl.rs:392-403`).
`tau.parallel()` blocks until *all* specs complete (`py_repl.rs:422-477`). For research,
the orchestrator needs to launch 5 threads, do other work, and collect results later.

### Gap 4: No document reactivity

Documents can be read and written, but there is no mechanism for a thread to be notified
when a document changes. Thread B cannot say "wait until document X has content, then
read it." The only option is blind polling, which wastes turns against the 25-turn limit.

### Gap 5: Episode injection is post-completion only

A thread can only receive another thread's compact trace after that thread finishes
(`thread.rs:301-314`). Running threads cannot share intermediate findings except through
documents -- which works, but requires threads to be explicitly programmed to write
findings as they go. There is no automatic "publish intermediate state" mechanism.

---

## Design

### Architecture Overview

```
tau session (long-lived)
|
+-- py_repl orchestrator (persistent Python kernel)
|   |
|   +-- Research State (disk-backed documents + workqueue JSON)
|   |
|   +-- Phase Controller (adaptive loop with checkpoint/replan)
|   |   - reads experiment configs
|   |   - dispatches batches via tau.launch()
|   |   - runs checkpoints after each batch
|   |   - queries reasoning model for adaptation decisions
|   |
|   +-- Synthesis Thread (periodic)
|       - reads all result documents
|       - fits scaling curves, identifies patterns
|       - writes findings to cross-cutting documents
|       - proposes next experiments to phase controller
|
+-- Worker Threads (parallel, in worktrees)
|   |
|   +-- Thread: "spice-mace-small-100k"
|   |   - generates/modifies config
|   |   - calls submit.py to launch Modal job
|   |   - polls status.py every 2-5 minutes
|   |   - calls fetch.py to collect results
|   |   - writes structured JSON to results/
|   |   - appends summary to document "spice-results"
|   |   - completes with key metrics as evidence
|   |
|   +-- Thread: "dna-bpe-transformer-100m"
|   |   - same pattern, different project
|   |
|   +-- Thread: "geneformer-reproduce-316m"
|       - builds data pipeline
|       - submits multi-hour training job
|       - monitors checkpoints
|       - runs evaluation on intermediate checkpoints
|
+-- Documents (disk-backed, shared mutable state)
    |
    +-- "spice-results"          (append-only metrics from chemistry threads)
    +-- "dna-scaling-results"    (append-only metrics from bio threads)
    +-- "cross-domain-findings"  (synthesis thread writes, others read)
    +-- "infrastructure-notes"   (lessons learned during pipeline building)
    +-- "experiment-queue"       (what to run next, managed by phase controller)
```

### Thread Lifecycle for Research

A research worker thread follows this pattern:

```
1. Read experiment config (YAML/JSON)
2. Check if result already exists (idempotency)
   - If yes: read result, write to doc, complete immediately
3. Submit job to Modal
   - `python scripts/submit.py --config <path>`
   - Capture job_id
4. Poll loop (turn-efficient):
   - `python scripts/status.py --job-id <id>` every 2-5 minutes
   - If running: sleep (via bash `sleep 120`), poll again
   - If failed: diagnose, maybe retry once
   - If complete: proceed to fetch
5. Fetch results
   - `python scripts/fetch.py --job-id <id>`
   - Parse JSON result
6. Write results
   - Save to results/ directory
   - Append summary to shared document
7. Complete with key metrics as evidence
```

This is ~15-30 turns for a successful run. With retries or debugging, up to ~80. The
current 25-turn limit is insufficient; 200 is safe for research threads.

### Communication Model

Threads communicate through three mechanisms, in order of preference:

**1. Documents (primary).** Shared mutable state for structured data. Thread A appends
results; Thread B reads the latest. Works for: metrics, findings, configuration updates.

**2. Episode injection (for sequential dependencies).** Thread B specifies
`episodes=["thread-a"]` to receive A's compact trace. Works for: building on prior work,
debugging failures, context transfer.

**3. Filesystem (for large artifacts).** Training checkpoints, datasets, and plots live
on disk. Threads reference paths. Works for: anything too large for documents.

Documents are the workhorse. The key insight is that threads should be *disciplined
about writing findings as they go*, not just at completion. A thread that discovers
"learning rate 3e-4 diverges for Mamba at 100M params" should immediately write that to
a document, not wait until it finishes.

### Persistence Model

```
.tau/
  documents/                  # Disk-backed virtual documents
    spice-results.md          # Survives session restart
    dna-scaling-results.md
    cross-domain-findings.md
  workqueue.json              # Phase controller state (already exists)
```

On document write/append: write to in-memory HashMap *and* flush to
`.tau/documents/{name}.md`. On session start: scan `.tau/documents/` and pre-populate
the HashMap. Documents are append-heavy and read-often; the disk write on every mutation
is acceptable (these are small text files, not training data).

### Adaptive Loop

The phase controller runs in py_repl and follows the supervised loop pattern with
research-specific adaptations:

```python
# Orchestrator main loop (py_repl)

experiment_queue = load_queue(".tau/workqueue.json")

for phase in experiment_queue.phases():
    # 1. Collect ready items (dependencies satisfied)
    batch = phase.ready_items()

    # 2. Launch workers (non-blocking)
    handles = []
    for item in batch:
        h = tau.launch(
            alias=item.id,
            task=make_worker_prompt(item),
            tools="full",
            worktree=True,
            timeout=item.timeout,
            max_turns=200,
        )
        handles.append(h)

    # 3. Wait for batch (with partial collection)
    results = tau.wait(handles, timeout=phase.timeout)

    # 4. Checkpoint: evaluate actual state
    for item, result in zip(batch, results):
        # Read result document for actual metrics
        metrics = tau.document("read", name=f"{item.id}-results")

        if result.status == "completed" and metrics_acceptable(metrics):
            experiment_queue.mark_done(item.id)
        elif result.status == "timed_out":
            experiment_queue.mark_retry(item.id, increase_timeout=True)
        else:
            # Ask reasoning model: retry, split, skip, or absorb?
            decision = tau.query(
                prompt=make_checkpoint_prompt(item, result, metrics),
                model="reasoning"
            )
            apply_decision(experiment_queue, item, decision)

    # 5. Persist queue state
    save_queue(experiment_queue, ".tau/workqueue.json")

    # 6. Run synthesis
    tau.thread(
        alias="synthesis",
        task="Read all result documents. Fit scaling curves. Identify patterns. Update 'cross-domain-findings' document.",
        tools="read",
        episodes=[item.id for item in batch if item.status == "done"],
        max_turns=50,
    )
```

---

## Tau Modifications

### Modification 1: Configurable max_turns per thread

**What**: Add `max_turns` parameter to the thread tool schema and wire it through to
agent construction.

**Where**: `coding-agent/src/tools/thread.rs`

**Changes**:

1. Add to parameter schema (after line 133):
   ```json
   {
     "name": "max_turns",
     "type": "integer",
     "description": "Maximum agent turns before timeout. Default: 25. Research threads may need 100-200."
   }
   ```

2. Parse from params (around line 215):
   ```rust
   let max_turns = params.get("max_turns")
       .and_then(|v| v.as_u64())
       .map(|v| v as usize)
       .unwrap_or(25);
   ```

3. Replace hard-coded value (line 374):
   ```rust
   // Before:
   max_turns: Some(25),
   // After:
   max_turns: Some(max_turns),
   ```

4. Add to py_repl dispatch: `max_turns` parameter passed through `dispatch_thread()`
   (already works -- params are forwarded directly to thread tool).

**Effort**: ~20 lines of code changes.

### Modification 2: Disk-backed documents

**What**: Persist documents to `.tau/documents/` on every write/append. Load from disk on
session start.

**Where**: `agent/src/orchestrator.rs`

**Changes**:

1. Add `documents_dir: Option<PathBuf>` to `OrchestratorState` (line 66).

2. Add constructor `with_documents_dir(path: PathBuf)`:
   - Create directory if not exists
   - Scan for existing `.md` files
   - Pre-populate `documents` HashMap from file contents

3. Modify `write_document()` (line 213):
   ```rust
   pub fn write_document(&self, name: &str, content: String) {
       let mut docs = self.documents.write().unwrap();
       docs.insert(name.to_string(), content.clone());
       if let Some(ref dir) = self.documents_dir {
           let path = dir.join(format!("{}.md", sanitize_filename(name)));
           let _ = std::fs::write(&path, &content);
       }
   }
   ```

4. Same pattern for `append_document()` (line 221).

5. Wire `documents_dir` from config in `agent_builder.rs`:
   ```rust
   let docs_dir = cwd.join(".tau/documents");
   orchestrator = OrchestratorState::with_documents_dir(docs_dir);
   ```

**Effort**: ~100 lines. The in-memory path is untouched; disk is a write-through layer.

### Modification 3: Non-blocking thread launch (tau.launch / tau.poll / tau.wait)

**What**: Add `launch()`, `poll()`, and `wait()` RPC methods to py_repl so the
orchestrator can manage threads without blocking.

**Where**: `coding-agent/src/tools/py_repl.rs`

**Changes**:

1. Add a `RunningThreads` map to `PyReplTool`:
   ```rust
   running: Arc<Mutex<HashMap<String, JoinHandle<ToolResult>>>>
   ```

2. `dispatch_launch()` (~30 lines):
   - Same setup as `dispatch_thread()` but spawns a tokio task
   - Stores the JoinHandle keyed by alias
   - Returns immediately with `{"launched": alias}`

3. `dispatch_poll()` (~20 lines):
   - Check if the JoinHandle for alias is finished
   - If done: remove from map, return result
   - If running: return `{"status": "running"}`

4. `dispatch_wait()` (~30 lines):
   - Takes list of aliases + optional timeout
   - `tokio::select!` across all handles (or timeout)
   - Returns results for completed threads, status for still-running

5. Register in dispatcher (line 351):
   ```rust
   "launch" => self.dispatch_launch(params).await,
   "poll" => self.dispatch_poll(params).await,
   "wait" => self.dispatch_wait(params).await,
   ```

6. Python-side API (in py_kernel.py):
   ```python
   def launch(self, alias, task, **kwargs):
       return self._rpc("launch", alias=alias, task=task, **kwargs)

   def poll(self, alias):
       return self._rpc("poll", alias=alias)

   def wait(self, aliases, timeout=None):
       return self._rpc("wait", aliases=aliases, timeout=timeout)
   ```

**Effort**: ~80 lines Rust + ~20 lines Python.

### Modification 4: Document watch/subscribe (optional, lower priority)

**What**: A tool that blocks until a document changes or contains a pattern.

**Where**: New file `coding-agent/src/tools/doc_watch.rs`, or extend `document.rs`.

**Mechanism**: Add a `tokio::sync::watch` channel per document in OrchestratorState.
When a document is written/appended, send a notification. The watch tool awaits the
notification with a timeout.

**API**:
```json
{
  "operation": "watch",
  "name": "spice-results",
  "timeout": 300,
  "pattern": "mace-large"
}
```

Returns the document content when it changes (or when `pattern` appears in it), or times
out.

**Why optional**: Documents + polling already work. This is an optimization to reduce
wasted turns. Implement after the core modifications are validated.

**Effort**: ~80 lines.

### Modification 5: Research workflow prompt template

**What**: A new workflow prompt (like `supervised.md`) tailored for ML research
orchestration.

**Where**: `coding-agent/prompts/orchestration/workflows/research.md`

**Contents**: The prompt teaches the orchestrator how to:
- Parse experiment configs (YAML grid definitions)
- Launch worker threads with appropriate timeouts and max_turns
- Use documents for inter-thread communication
- Run synthesis threads periodically
- Checkpoint with reasoning model for adaptation
- Handle Modal job lifecycle (submit/poll/fetch)
- Persist state for session resume

This is where the research-specific knowledge lives. The tau code changes are generic;
the prompt makes them research-aware.

**Effort**: ~300 lines of markdown.

---

## Experiment Infrastructure Contract

For tau to orchestrate experiments, each research project must provide a standardized
interface. This section defines that contract.

### Directory Structure

```
project/
  experiments/
    {experiment-name}/
      configs/                  # One YAML per grid cell
        bpe-transformer-10m.yaml
        bpe-transformer-100m.yaml
        ...
      scripts/
        submit.py               # Launch a job, print job_id
        status.py               # Check job status, print JSON
        fetch.py                # Download results, print path
      results/                  # One JSON per completed run
        bpe-transformer-10m.json
        ...
      analysis/
        fit_scaling.py          # Power-law fitting
        plot.py                 # Generate figures
      grid.yaml                 # Grid definition (optional)
      README.md                 # Experiment description
```

### Config Schema

Each experiment point is a YAML file specifying all parameters needed to reproduce the
run:

```yaml
# configs/bpe-transformer-100m.yaml
experiment: dna-tokenization-scaling
cell_id: bpe-transformer-100m

# Model
architecture: transformer
hidden_dim: 768
num_layers: 12
num_heads: 12
vocab_size: 8192
target_params: 100_000_000

# Data
tokenization: bpe
dataset: human_genome
max_tokens: 10_000_000_000

# Training
learning_rate: 3.0e-4
warmup_steps: 1000
max_epochs: 50
batch_size: 64
gradient_accumulation: 4

# Compute
gpu_type: a100
num_gpus: 1
estimated_hours: 4.0

# Evaluation
eval_benchmarks:
  - gue
eval_checkpoints: [10, 25, 50]
```

### Grid Definition (Optional)

For combinatorial experiments, a `grid.yaml` defines the sweep:

```yaml
experiment: dna-tokenization-scaling
base_config:
  dataset: human_genome
  max_epochs: 50
  learning_rate: 3.0e-4
  gpu_type: a100

axes:
  architecture:
    - {name: transformer, hidden_dim: 768, num_layers: 12, num_heads: 12}
    - {name: mamba, d_state: 16, d_conv: 4, expand: 2}
    - {name: striped_hyena, hidden_dim: 768, num_layers: 12}
  tokenization:
    - {name: bpe, vocab_size: 8192}
    - {name: kmer, k: 6}
    - {name: character, vocab_size: 5}
  model_size:
    - {name: 10m, target_params: 10_000_000}
    - {name: 30m, target_params: 30_000_000}
    - {name: 100m, target_params: 100_000_000}
    - {name: 300m, target_params: 300_000_000}

phases:
  - name: pilot
    cells: [[transformer, bpe, 10m], [mamba, character, 10m]]
    gate: "all cells complete with val_loss < 5.0"
  - name: core
    cells: [[transformer, bpe, *], [mamba, character, *]]
    gate: "R-squared of power-law fit > 0.90"
  - name: full
    cells: "*"
```

### Submit/Status/Fetch Scripts

These scripts are the contract between tau and the compute backend. Each is a standalone
CLI tool.

**submit.py**:
```
Usage: python scripts/submit.py --config configs/bpe-transformer-100m.yaml
Output: {"job_id": "abc123", "estimated_hours": 4.0}
Exit 0 on success, non-zero on failure.
```

Behavior:
- Reads config YAML
- Validates parameters
- Submits job to Modal (or other backend)
- Prints JSON with job_id to stdout
- Idempotent: if a job for this config is already running, returns its job_id

**status.py**:
```
Usage: python scripts/status.py --job-id abc123
Output: {"status": "running", "epoch": 23, "train_loss": 2.5, "elapsed_hours": 1.2}
       {"status": "completed", "result_path": "results/bpe-transformer-100m.json"}
       {"status": "failed", "error": "OOM at epoch 12", "last_checkpoint": "ckpt-11.pt"}
```

Status values: `pending`, `running`, `completed`, `failed`, `cancelled`.

**fetch.py**:
```
Usage: python scripts/fetch.py --job-id abc123
Output: {"result_path": "results/bpe-transformer-100m.json", "checkpoints": [...]}
Exit 0 on success, non-zero if job not completed.
```

Behavior:
- Downloads result JSON from Modal volume
- Downloads checkpoints if requested
- Writes to local `results/` directory

### Result Schema

Every training run produces a JSON result file:

```json
{
  "experiment": "dna-tokenization-scaling",
  "cell_id": "bpe-transformer-100m",
  "config_hash": "a1b2c3d4",

  "model": {
    "architecture": "transformer",
    "tokenization": "bpe",
    "actual_params": 98_500_000,
    "vocab_size": 8192
  },

  "training": {
    "status": "completed",
    "epochs_completed": 50,
    "final_train_loss": 2.14,
    "final_val_loss": 2.31,
    "best_val_loss": 2.28,
    "best_epoch": 43,
    "tokens_seen": 10_000_000_000,
    "wall_time_hours": 3.8,
    "gpu_hours": 3.8
  },

  "evaluation": {
    "gue_overall": 0.72,
    "gue_per_task": {
      "promoter_detection": 0.87,
      "tf_binding": 0.73,
      "splice_site": 0.81,
      "chromatin_accessibility": 0.64
    }
  },

  "compute": {
    "gpu_type": "a100",
    "num_gpus": 1,
    "modal_job_id": "abc123",
    "peak_memory_gb": 38.2,
    "cost_estimate_usd": 11.40
  },

  "reproducibility": {
    "git_commit": "a1b2c3d",
    "random_seed": 42,
    "framework_versions": {
      "torch": "2.5.0",
      "transformers": "4.46.0"
    }
  },

  "timestamp": "2026-04-10T14:23:00Z"
}
```

Required fields: `experiment`, `cell_id`, `model`, `training.status`, `training.final_val_loss`.
Everything else is optional but strongly encouraged.

---

## Communication Patterns

### Pattern 1: Append-Only Result Log

Used for: accumulating metrics across a grid.

```
Document: "spice-results"

Thread "spice-small-100k" appends:
  [spice-small-100k] E_rmse=3.45 meV/atom, F_rmse=12.1 meV/A, 1.2 GPU-hrs

Thread "spice-small-250k" appends:
  [spice-small-250k] E_rmse=2.89 meV/atom, F_rmse=10.3 meV/A, 2.4 GPU-hrs

Synthesis thread reads the full document, fits L(D) = a * D^(-beta).
```

Documents are text, not structured data. This is intentional: LLM threads read and
write natural language more reliably than JSON. The synthesis thread parses the
structured parts it needs.

### Pattern 2: Finding Broadcast

Used for: sharing discoveries that affect other threads' work.

```
Document: "infrastructure-notes"

Thread "geneformer-reproduce" writes:
  CELLxGENE Census API streams at ~500 cells/sec. For 95M cells,
  pre-download to Parquet is mandatory. h5ad concurrent reads fail
  with HDF5 locking errors. Use cellxgene_census.get_anndata() with
  obs_value_filter to shard by tissue type.

Thread "geneformer-scale-1b" reads this before starting its data pipeline.
```

### Pattern 3: Coordination Handoff

Used for: sequential dependencies between experiment phases.

```
Document: "phase-gate-dna-pilot"

Synthesis thread writes:
  PHASE 1 COMPLETE. 4/4 pilot runs finished.
  BPE-Transformer-10M: val_loss=3.82
  Char-Mamba-10M: val_loss=4.01
  Power-law fit: not enough points, but losses are reasonable.
  RECOMMENDATION: Proceed to Phase 2 core grid.

Phase controller reads this, decides to launch Phase 2.
```

### Pattern 4: Cross-Project Findings

Used for: insights that span jenny-doudna and jonny-von-neuman.

```
Document: "cross-domain-findings"

Synthesis thread writes:
  Updated 2026-04-12.
  Chemistry (SPICE): alpha_N = 0.065, alpha_D = 0.42. Data scaling dominates.
  DNA (pilot): alpha_N = 0.071 (2 points only). Consistent with ESM-2 protein.
  Hypothesis: molecular domains (protein, DNA) scale similarly; single-cell
  may be different due to fundamentally different data structure (expression
  profiles vs sequences).
```

---

## Concrete Scenarios

### Scenario 1: SPICE Scaling Grid (jonny-von-neuman)

This is the proof-of-concept. Infrastructure already exists: `train_grid.py` submits
Modal jobs, `analyze_scaling.py` fits power laws. Gap: manual wave execution.

**What changes**:
1. Refactor `train_grid.py` into submit/status/fetch scripts
2. Add YAML configs for each (model_size, data_size) cell
3. Add result JSON schema (close to existing `summary.json`)

**Orchestration**:
```python
# Phase controller
configs = glob("experiments/02-spice-scaling/configs/*.yaml")
for batch in chunk_by_wave(configs):  # small, medium, large
    handles = [tau.launch(alias=c.stem, task=worker_prompt(c), ...) for c in batch]
    results = tau.wait(handles, timeout=28800)  # 8 hours
    # Checkpoint: fit partial curves, decide if wave is worth continuing
    tau.thread(alias="spice-checkpoint", task="Fit scaling curves from 'spice-results' doc...")
```

**Estimated effort to adapt**: 1 day (infra already 80% there).

### Scenario 2: DNA Tokenization x Architecture Grid (jenny-doudna)

Infrastructure needs to be built from scratch. 36 cells, 3 architectures, 3
tokenizations, 4 sizes.

**What needs building**:
1. Unified BERT/Mamba/StripedHyena training harness (parameterized by config)
2. BPE, k-mer, character tokenizers for DNA
3. Modal submission scripts (submit/status/fetch)
4. GUE evaluation harness
5. YAML configs for all 36 cells (generated from grid.yaml)

**Orchestration**:
```python
# Phase 0: Pipeline validation (1 cell)
tau.thread(alias="dna-infra", task="Build training harness...", worktree=True, max_turns=200, timeout=3600)

# Phase 1: Pilot (4 cells)
pilots = ["bpe-transformer-10m", "char-mamba-10m", "bpe-mamba-10m", "char-transformer-10m"]
handles = [tau.launch(alias=p, task=worker_prompt(p), max_turns=200, timeout=14400) for p in pilots]
results = tau.wait(handles)

# Gate: check if losses are reasonable
gate = tau.query("Given pilot results: ... Should we proceed to full grid?", model="reasoning")

# Phase 2: Full grid (36 cells, batched by cost)
# ...
```

**Estimated effort to adapt**: 1-2 weeks (training harness is the bulk).

### Scenario 3: Cross-Project Synthesis

The most ambitious scenario: threads from both projects share findings through a common
synthesis layer.

**Prerequisites**:
- Both projects have running experiments producing results
- A shared document namespace (or documents prefixed by project)

**Orchestration**:
```python
# Run periodically (every few hours or after each batch completes)
tau.thread(
    alias="cross-domain-synthesis",
    task="""
    Read documents: 'spice-results', 'dna-scaling-results', 'geneformer-results'.
    For each domain, fit L(N) = a * N^(-alpha) if >= 3 data points.
    Compare alpha values across domains.
    Update document 'cross-domain-findings' with:
    - Per-domain scaling exponents with confidence intervals
    - Cross-domain comparison and ranking
    - Recommendations for where additional compute would be most informative
    Reference: existing Phase 0 analysis shows protein alpha~0.07, DNA alpha~0.03.
    """,
    tools="read",
    max_turns=50,
    timeout=600,
)
```

This thread reads from both projects' result documents and produces a unified analysis.
It does not need worktree isolation (read-only). It does not need to submit jobs. It is
a pure analysis/synthesis role.

---

## Implementation Plan

### Phase 1: Core Tau Changes (unblocks everything)

| Task | File | Lines | Priority |
|---|---|---|---|
| Configurable `max_turns` per thread | `thread.rs` | ~20 | P0 |
| Disk-backed documents | `orchestrator.rs`, `agent_builder.rs` | ~100 | P0 |
| `tau.launch()` / `tau.poll()` / `tau.wait()` | `py_repl.rs`, `py_kernel.py` | ~100 | P0 |

These three changes are independent and can be implemented in parallel. Together they
unblock the research workflow.

### Phase 2: Research Workflow Prompt

| Task | File | Lines | Priority |
|---|---|---|---|
| Research orchestration prompt | `prompts/orchestration/workflows/research.md` | ~300 | P1 |
| Example configs and grid.yaml | `prompts/orchestration/examples/` | ~100 | P1 |

The prompt teaches the LLM orchestrator how to manage research workflows using the
primitives from Phase 1.

### Phase 3: Proof of Concept (SPICE scaling)

| Task | Where | Effort | Priority |
|---|---|---|---|
| Refactor train_grid.py into submit/status/fetch | jonny-von-neuman | 1 day | P1 |
| YAML configs for 12 cells | jonny-von-neuman | 2 hours | P1 |
| Standardize result JSON | jonny-von-neuman | 2 hours | P1 |
| End-to-end test: tau orchestrates full SPICE grid | tau + jonny-von-neuman | 1 day | P1 |

### Phase 4: Jenny-Doudna Bootstrap

| Task | Where | Effort | Priority |
|---|---|---|---|
| Unified training harness | jenny-doudna | 1-2 weeks | P2 |
| Tokenizer implementations | jenny-doudna | 2-3 days | P2 |
| submit/status/fetch scripts | jenny-doudna | 1 day | P2 |
| GUE evaluation harness | jenny-doudna | 2-3 days | P2 |
| YAML configs for 36 cells | jenny-doudna | Half day | P2 |
| Pilot run: 4-cell grid | jenny-doudna | 1-2 days | P2 |

### Phase 5: Cross-Project Synthesis

| Task | Where | Effort | Priority |
|---|---|---|---|
| Synthesis thread prompt | tau | Half day | P3 |
| Cross-domain analysis module | shared | 1-2 days | P3 |
| End-to-end: both projects feeding synthesis | all three repos | 1 day | P3 |

### Phase 6: Polish (optional)

| Task | File | Effort | Priority |
|---|---|---|---|
| Document watch/subscribe | `orchestrator.rs`, `document.rs` | 1 day | P4 |
| TUI research dashboard | `coding-agent/src/tui/` | 2-3 days | P4 |
| Cost tracking integration | `py_repl.rs` | 1 day | P4 |

---

## Open Questions

1. **Single tau session or multiple?** Should one tau session orchestrate both projects,
   or one session per project with a shared document directory? Single session is simpler
   but risks context bloat. Separate sessions with a shared `.tau/documents/` directory
   might be cleaner.

2. **Document namespacing.** Should documents be flat (`"spice-results"`) or namespaced
   (`"chemistry/spice-results"`)? Flat is simpler for cross-project synthesis. Namespaced
   scales better if the document count grows large.

3. **How much to trust the synthesis thread?** The synthesis thread fits scaling curves
   and makes recommendations. Should its recommendations auto-execute (fully autonomous)
   or require human approval (checkpoint gate)? Start with human gates, relax as
   confidence builds.

4. **Cost controls.** A runaway orchestrator could burn through GPU budget. Should tau
   enforce a cost ceiling? This probably belongs in the submit.py layer (reject jobs if
   cumulative cost exceeds budget), not in tau itself.

5. **Multi-host orchestration.** Current design is single-host (one tau process). If
   experiments span multiple machines (e.g., on-prem cluster + Modal cloud), document
   synchronization becomes non-trivial. Out of scope for now; Modal handles the multi-host
   compute.

---

## Appendix: Existing Experiment Infrastructure Audit

### jonny-von-neuman (Chemistry)

| Component | Status | Gap |
|---|---|---|
| Grid configs | Inline Python dicts in `train_grid.py` | Need YAML externalization |
| Job submission | `modal run train_grid.py --wave small` | Need submit.py wrapper |
| Status checking | Manual (Modal dashboard) | Need status.py |
| Result fetching | Inline in train_grid.py | Need fetch.py |
| Result format | `summary.json` per run | Close to spec; add metadata fields |
| Analysis | `analyze_scaling.py` with power-law fitting | Works as-is |
| Data pipeline | `fetch_spice.py` + `prepare_splits.py` | Complete, idempotent |

### jenny-doudna (Bio)

| Component | Status | Gap |
|---|---|---|
| Grid configs | Markdown spec only | Everything needs building |
| Job submission | None | Needs training harness + submit.py |
| Status checking | None | Needs status.py |
| Result fetching | None | Needs fetch.py |
| Result format | None | Needs definition |
| Analysis | `cross_domain_scaling_plot.py` (Phase 0 only) | Needs per-experiment analysis |
| Data pipeline | None for training | Needs tokenizers, dataloaders, CELLxGENE pipeline |

### Commonalities

Both projects converge on the same pattern:
- **Grid-based experiment definition** (combinatorial sweep over axes)
- **Modal as compute backend** (A100/H100, volumes for persistence)
- **Power-law fitting as core analysis** (L(N), L(D) with exponent extraction)
- **Phase-gated execution** (validate pipeline before scaling up)
- **Per-run JSON results** (structured metrics + metadata)

The contract defined in this spec formalizes what jonny-von-neuman does ad-hoc and what
jenny-doudna plans to build.
