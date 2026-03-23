# Harness Benchmark Template

Standard patterns for tau microbenchmarks. Every benchmark under `benchmarks/`
follows this template for consistency, code reuse, and composability.

Patterns adapted from [edit-bench](~/projects/edit-bench/), our first benchmark
suite. edit-bench proved out: RPC session management, fixture-per-task layout,
normalization-before-comparison verification, and JSON+markdown reporting.

---

## Benchmark types

### Offline (no API calls)

Pure computation benchmarks that evaluate algorithms against a corpus.
No model calls, no session management, zero cost. Run in seconds.

Examples: `fuzzy-match`, `fuzzy-false-positive`

Pattern: generate corpus -> run matchers/algorithms -> score -> report

### Online (model-in-loop)

Benchmarks that spawn tau sessions and measure model behavior under different
configurations. Require API keys, cost money, take minutes to hours.

Examples: `fuzzy-e2e`, `post-edit-diagnostics`, `compaction-*`, `parallel-ops`,
`subagent-decomposition`, `todo-tracking`

Pattern: generate fixtures -> run tau sessions -> verify output -> score -> report

Most online benchmarks are **A/B tests**: same tasks, different tau
configurations, compare outcomes.

---

## Directory structure

```
benchmarks/{name}/
├── SPEC.md           # Design specification (this template)
├── generate.py       # Corpus/fixture generation
├── run.py            # Main benchmark runner
├── corpus/           # Generated test data (offline, gitignored)
│   └── README.md
├── fixtures/         # Task fixtures (online, may be committed)
│   └── {task_id}/
│       ├── input/
│       ├── expected/
│       ├── prompt.md
│       └── metadata.json
└── results/          # Benchmark output (gitignored)
```

Not every benchmark needs all of these. Offline benchmarks skip `fixtures/`.
Online benchmarks without file-output verification skip `expected/`.

---

## Shared infrastructure (`benchmarks/shared/`)

Common code imported by online benchmarks.

### TauSession (`shared/session.py`)

Wraps `tau serve` JSON-RPC 2.0 for persistent multi-turn sessions.

```python
class TauSession:
    """Context manager for a tau serve session."""

    def __init__(self, model: str, cwd: Path,
                 tools: list[str] | None = None,
                 edit_mode: str = "replace",
                 timeout: int = 120): ...

    def start(self) -> None:
        """Spawn tau serve, send initialize RPC."""

    def send(self, prompt: str) -> SessionResult:
        """Send prompt, block until idle. Returns result with token usage."""

    def shutdown(self) -> None: ...
    def __enter__(self) -> TauSession: ...
    def __exit__(self, ...) -> None: ...


@dataclass
class SessionResult:
    output: str
    input_tokens: int
    output_tokens: int
    tool_calls: int
    wall_clock_ms: int
```

Port from: `edit-bench/edit_bench/rpc.py` (TauRpcClient). The protocol:
1. Spawn `tau serve --cwd CWD --model MODEL --tools TOOLS`
2. Send JSON-RPC `initialize`
3. Send `session/send` with prompt
4. Wait for `session.status` notification with `type=idle`
5. Read usage from notification payload

### BenchConfig (`shared/config.py`)

```python
@dataclass
class BenchConfig:
    model: str = "claude-sonnet-4-6"
    edit_mode: str = "replace"
    runs_per_task: int = 1
    timeout: int = 120          # seconds per task
    concurrency: int = 4
    max_attempts: int = 1       # verification retries
    tau_binary: str = "tau"
    output_dir: Path = Path("results")
```

### TaskResult (`shared/result.py`)

```python
@dataclass
class TaskResult:
    task_id: str
    variant: str                # A/B config name
    run_index: int
    success: bool
    wall_clock_ms: int
    input_tokens: int
    output_tokens: int
    turns: int                  # LLM round-trips
    tool_calls: int
    error: str | None = None
    metadata: dict = field(default_factory=dict)
```

The `variant` field is key for A/B tests -- it identifies which configuration
produced this result.

### Reporter (`shared/reporter.py`)

```python
class Reporter:
    def __init__(self, benchmark_name: str,
                 results: list[TaskResult],
                 config: BenchConfig): ...

    def summary(self) -> dict: ...
    def by_category(self, key: str) -> dict[str, dict]: ...
    def by_variant(self) -> dict[str, dict]: ...
    def markdown(self) -> str: ...
    def json(self) -> str: ...
    def write(self, output_dir: Path) -> None:
        """Write report.md + report.json."""
```

### Verifier (`shared/verifier.py`)

For benchmarks that compare file output against expected state.

```python
class Verifier:
    def compare(self, actual: Path, expected: Path) -> VerifyResult: ...
    def diff(self, actual: Path, expected: Path) -> str: ...
```

Normalization pipeline (from edit-bench):
1. CRLF -> LF, strip trailing whitespace
2. Collapse runs of 3+ blank lines -> 2
3. Format with language formatter (ruff, rustfmt, gofmt, prettier)
4. Exact text comparison

---

## Fixture formats

### Offline corpus (JSON array)

```json
[
  {
    "id": "category-0001",
    "category": "trailing-ws",
    "inputs": { "file_content": "...", "old_string": "..." },
    "ground_truth": { "start_line": 10, "end_line": 15, "matched_text": "..." },
    "notes": "Trailing whitespace stripped"
  }
]
```

### Online fixtures (directory per task)

```
{task_id}/
├── input/            # Starting workspace
├── expected/         # Target state (optional, for verification)
├── prompt.md         # Task description
└── metadata.json     # Category, difficulty, context
```

metadata.json:

```json
{
  "category": "rename-type",
  "difficulty": "medium",
  "language": "rust",
  "description": "Rename struct field and propagate",
  "expected_changes": { "files_modified": 3, "lines_changed": 12 }
}
```

---

## A/B test pattern

Most online benchmarks compare tau under different configurations. The runner
iterates over variants:

```python
@dataclass
class Variant:
    name: str
    description: str
    tau_config: dict        # overrides: edit_mode, system_prompt, tools, etc.

variants = [
    Variant("baseline", "No diagnostics", {}),
    Variant("post-edit-check", "Compiler check after edit",
            {"post_edit_diagnostics": True}),
]

for variant in variants:
    for task in tasks:
        for run_idx in range(config.runs_per_task):
            result = run_task(task, variant, run_idx)
            results.append(result)
```

Reports slice by variant, showing per-variant pass rates, token usage, and
timing. This makes A/B comparison the default output.

---

## Retry strategy

For online benchmarks with `max_attempts > 1`:

```
attempt 1: send task prompt
  -> verify output
    -> pass: record success
    -> fail: build retry context (diff + hints)
      attempt 2: send retry context
        -> verify
          -> pass: record success (with attempt_count=2)
          -> fail: record failure
```

Retry context includes the diff between actual and expected output, plus
any metadata about what went wrong. This mirrors edit-bench's approach.

---

## CLI conventions

### Generate (offline)
```bash
uv run python generate.py <source_dir> -o corpus/<name>.json \
    [--lang rust] [--max-cases 200] [--seed 42]
```

### Generate (online)
```bash
uv run python generate.py <source_dir> -o fixtures/ \
    [--lang rust] [--max-tasks 20] [--difficulty hard]
```

### Run (offline)
```bash
uv run python run.py corpus/<name>.json \
    [--matchers exact normalized levenshtein-92] \
    [--json] [-o results/report.json]
```

### Run (online)
```bash
uv run python run.py fixtures/ \
    --model claude-sonnet-4-6 \
    [--edit-mode replace] \
    [--runs 3] \
    [--timeout 120] \
    [--concurrency 4] \
    [--variants baseline,with-diagnostics] \
    [-o results/]
```

Standard flags across all online runners:
- `--model`: Model identifier
- `--edit-mode`: `replace` | `hashline`
- `--runs`: Runs per task per variant (default: 1)
- `--timeout`: Seconds per task (default: 120)
- `--concurrency`: Parallel tasks (default: 4)
- `--variants`: Comma-separated variant names to run
- `-o, --output`: Output directory
- `--json`: Machine-readable output to stdout
- `--filter`: Filter tasks by category/difficulty

---

## Reporting standard

### JSON report

```json
{
  "benchmark": "post-edit-diagnostics",
  "timestamp": "2026-03-23T10:00:00Z",
  "config": { "model": "claude-sonnet-4-6", "edit_mode": "replace", "runs": 3 },
  "summary": {
    "total_runs": 36,
    "passed": 30,
    "pass_rate": 0.833,
    "total_input_tokens": 450000,
    "total_output_tokens": 25000,
    "total_time_ms": 180000
  },
  "by_variant": {
    "baseline": { "total": 18, "passed": 12, "pass_rate": 0.667 },
    "with-diagnostics": { "total": 18, "passed": 18, "pass_rate": 1.0 }
  },
  "by_category": { ... },
  "results": [ ... ]
}
```

### Markdown report

```markdown
# {Benchmark Name} Results — {timestamp}

## Summary
| Metric | Value |
|--------|-------|

## By Variant
| Variant | Tasks | Passed | Rate | Avg Tokens | Avg Time |
|---------|-------|--------|------|------------|----------|

## By Category
| Category | Variant A | Variant B | Delta |
|----------|-----------|-----------|-------|

## Failures (first 10)
| Task | Variant | Error |
|------|---------|-------|
```

---

## Writing a new benchmark

1. Create `benchmarks/{name}/` directory
2. Write `SPEC.md` following the spec template below
3. Implement `generate.py` (if corpus/fixtures needed)
4. Implement `run.py` importing from `shared/` as needed
5. Test with a small corpus/fixture set
6. Run full benchmark, write results to `results/`
7. Update `SPEC.md` with findings and revised decisions

### SPEC.md template

```markdown
# {Benchmark Name}

Phase: {1-4} | Type: {offline|online} | Cost: ${est} | Time: {est}

## What it measures
## Why it matters for tau
## Prerequisites
## Fixtures
## Procedure
## Variants / run matrix
## Metrics and scoring
## Decision it informs
## Architecture (code structure, shared code, est. LOC)
## CLI
```
