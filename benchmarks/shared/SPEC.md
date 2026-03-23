# Shared Benchmark Infrastructure

Phase: 0 (build before Phase 2 benchmarks) | Type: library | Cost: $0 | Time: ~1 day

## What it provides

Common Python modules imported by all online (model-in-loop) benchmarks.
Eliminates duplication of session management, configuration, result collection,
and reporting across 7+ benchmark runners.

## Why it matters for tau

Without shared infrastructure, each benchmark reimplements tau session spawning,
token accounting, and result reporting. edit-bench already proved these patterns;
this package ports and generalizes them.

## Source material

Primary reference: `~/projects/edit-bench/edit_bench/`

| edit-bench module | shared module | What we port |
|-------------------|---------------|--------------|
| `rpc.py` (TauRpcClient) | `session.py` (TauSession) | JSON-RPC 2.0 client, spawn/shutdown, send/wait-for-idle |
| runner.py:BenchmarkConfig | `config.py` (BenchConfig) | Config dataclass with CLI defaults |
| runner.py:TaskResult | `result.py` (TaskResult) | Result dataclass with variant field |
| runner.py:generate_report() | `reporter.py` (Reporter) | JSON + markdown report generation |
| runner.py:verify_output() | `verifier.py` (Verifier) | Normalization pipeline + diff |

## Modules

### `session.py` — TauSession

Context manager wrapping `tau serve` JSON-RPC 2.0.

Key differences from edit-bench's TauRpcClient:
- Supports **variant configuration**: accepts a `Variant` object that can
  override model, edit_mode, tools, system prompt additions, and feature flags
- Adds **turn counting**: tracks number of send/receive cycles
- Adds **tool call counting**: parsed from session notifications

Methods:
- `start()` — spawn subprocess, initialize
- `send(prompt) -> SessionResult` — send prompt, block for idle
- `shutdown()` — graceful termination
- Context manager protocol

Does NOT include retry logic — that's benchmark-specific (different benchmarks
have different feedback strategies).

### `config.py` — BenchConfig

```python
@dataclass
class BenchConfig:
    model: str = "claude-sonnet-4-6"
    edit_mode: str = "replace"
    runs_per_task: int = 1
    timeout: int = 120
    concurrency: int = 4
    max_attempts: int = 1
    tau_binary: str = "tau"
    output_dir: Path = Path("results")

    @classmethod
    def from_cli(cls, args: argparse.Namespace) -> BenchConfig: ...

    def add_cli_args(parser: argparse.ArgumentParser) -> None: ...
```

The `add_cli_args` / `from_cli` pair gives every runner the same standard flags
without duplicating argument definitions.

### `result.py` — TaskResult

```python
@dataclass
class TaskResult:
    task_id: str
    variant: str
    run_index: int
    success: bool
    wall_clock_ms: int
    input_tokens: int
    output_tokens: int
    turns: int
    tool_calls: int
    error: str | None = None
    metadata: dict = field(default_factory=dict)
```

The `metadata` dict holds benchmark-specific fields:
- fuzzy-e2e: `edit_success_rate`, `retry_count`
- compaction-recall: `recall_accuracy`, `false_positives`
- post-edit-diagnostics: `cycle_count`, `diagnostic_tokens`

### `reporter.py` — Reporter

Generates JSON and markdown reports from a list of TaskResults.

Standard slicing:
- **by_variant**: A/B comparison (the primary view for most benchmarks)
- **by_category**: per-category breakdown (task metadata key)
- **by_difficulty**: easy/medium/hard if tasks have difficulty scores

Markdown output follows the format in TEMPLATE.md.

### `verifier.py` — Verifier

Normalization-before-comparison pipeline for benchmarks that check file output.

Normalization steps:
1. Line ending normalization (CRLF -> LF)
2. Trailing whitespace stripping
3. Blank line collapsing (3+ -> 2)
4. Language-specific formatting (ruff, rustfmt, gofmt, prettier)
5. Exact text comparison

Only needed by: `fuzzy-e2e`, `subagent-decomposition`.

### `variants.py` — Variant

```python
@dataclass
class Variant:
    name: str
    description: str
    edit_mode: str = "replace"
    tools: list[str] | None = None
    system_prompt_suffix: str = ""
    tau_config_overrides: dict = field(default_factory=dict)
```

Used by A/B test runners to define the configurations being compared.

### `store.py` — ResultStore

**Implemented.** Handles local result persistence and optional remote sync.

```python
class ResultStore:
    def __init__(self, benchmark: str): ...
    def save(self, report: dict) -> str:   # local write, returns run_id
    def push(self, run_id: str = None):    # upload to R2 via rclone
    def pull(self, run_id: str = None):    # download from R2
    def ls(benchmark: str = None):         # list local + remote runs
```

Features:
- Auto-enriches reports with `run_id`, `timestamp`, `host`, `git_sha`, `git_dirty`
- Local-only by default (no config needed)
- Remote sync via rclone when `TAU_BENCH_REMOTE` env var is set
- CLI interface: `python -m shared.store save|push|pull|ls`

See TEMPLATE.md "Result storage and querying" for full setup and DuckDB
query examples.

## Architecture

```
shared/
├── __init__.py
├── store.py          # ResultStore (implemented)
├── session.py        # TauSession
├── config.py         # BenchConfig
├── result.py         # TaskResult, SessionResult
├── reporter.py       # Reporter
├── verifier.py       # Verifier
└── variants.py       # Variant
```

Estimated LOC: ~800 total

- store.py: ~200 (implemented)
- session.py: ~200 (most complex — subprocess + JSON-RPC protocol)
- config.py: ~60
- result.py: ~40
- reporter.py: ~200 (markdown table generation, JSON serialization)
- verifier.py: ~80
- variants.py: ~20

## Dependencies

Zero external dependencies beyond Python stdlib. Formatters (ruff, rustfmt, etc.)
are optional and detected at runtime.

## Import pattern

Benchmarks import shared code via path manipulation:

```python
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.session import TauSession
from shared.config import BenchConfig
from shared.result import TaskResult
from shared.reporter import Reporter
```

Alternative: install `tau-benchmarks` in dev mode (`uv pip install -e .`) and
restructure as a proper package. Defer this until we have 3+ online benchmarks
actually using the shared code.

## Build order

1. ~~`store.py`~~ — **done**
2. `session.py` — needed by all online benchmarks, port from edit-bench first
3. `config.py` + `result.py` — trivial dataclasses
4. `reporter.py` — needed before first benchmark run for output
5. `verifier.py` — needed only when fuzzy-e2e or subagent-decomposition ships
6. `variants.py` — trivial dataclass, build alongside first A/B benchmark
