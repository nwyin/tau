"""Post-edit diagnostics benchmark runner.

Measures edit-test-fix cycle count under different diagnostic configurations.
Spawns tau sessions for each variant x task x run combination, copies fixture
workspaces to temp dirs, and verifies output against expected files.

Usage:
    uv run python run.py fixtures/ --model claude-sonnet-4-6 --variants no-diag,prompt-check --runs 3 -o results/
"""

from __future__ import annotations

import argparse
import shutil
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.config import BenchConfig
from shared.reporter import Reporter
from shared.result import TaskResult
from shared.session import TauSession, SessionResult
from shared.store import ResultStore
from shared.variants import Variant

from variants import get_variants

BENCHMARK_NAME = "post-edit-diagnostics"


# ---------------------------------------------------------------------------
# Fixture discovery
# ---------------------------------------------------------------------------


def discover_tasks(fixtures_dir: Path) -> list[dict]:
    """Find all task directories under fixtures_dir.

    Each task directory must contain input/, expected/, and prompt.md.
    Returns a list of dicts with keys: id, prompt, input_dir, expected_dir, language.
    """
    tasks: list[dict] = []
    for task_dir in sorted(fixtures_dir.iterdir()):
        if not task_dir.is_dir():
            continue
        prompt_file = task_dir / "prompt.md"
        input_dir = task_dir / "input"
        expected_dir = task_dir / "expected"
        if not (prompt_file.exists() and input_dir.exists() and expected_dir.exists()):
            continue
        language = _detect_language(input_dir)
        tasks.append(
            {
                "id": task_dir.name,
                "prompt": prompt_file.read_text(),
                "input_dir": input_dir,
                "expected_dir": expected_dir,
                "language": language,
            }
        )
    return tasks


def _detect_language(input_dir: Path) -> str:
    """Detect primary language from file extensions in a directory."""
    extensions: dict[str, str] = {}
    for f in input_dir.rglob("*"):
        if f.is_file():
            ext = f.suffix.lower()
            extensions[ext] = extensions.get(ext, 0) + 1  # type: ignore[assignment]
    ext_to_lang = {".rs": "rust", ".ts": "typescript", ".py": "python", ".go": "go", ".js": "javascript"}
    best_ext = max(extensions, key=lambda e: extensions[e], default="")
    return ext_to_lang.get(best_ext, "unknown")


# ---------------------------------------------------------------------------
# Cycle counting
# ---------------------------------------------------------------------------


def count_cycles(session_result: SessionResult) -> int:
    """Estimate edit-error-fix cycles from session result.

    A cycle is defined as: model edits a file, encounters an error (either
    via compiler output or by noticing something wrong), then edits again
    to fix it. We approximate this by looking at tool_calls — each pair
    of edit tool calls without a successful completion in between suggests
    a fix cycle.

    Minimum 1 cycle (the initial edit pass itself).
    """
    # Without access to the full conversation trace, we estimate cycles
    # from the number of tool calls. A clean pass uses ~N tool calls
    # (one per file). Extra tool calls indicate fix cycles.
    # Heuristic: cycles = max(1, tool_calls // 3) — each cycle is roughly
    # read + edit + verify.
    if session_result.tool_calls <= 0:
        return 1
    return max(1, session_result.tool_calls // 3)


# ---------------------------------------------------------------------------
# Verification
# ---------------------------------------------------------------------------


def verify_output(workspace: Path, expected_dir: Path) -> tuple[bool, str]:
    """Compare workspace files against expected output.

    Returns (success, error_message). Compares only files present in
    expected_dir — extra files in workspace are ignored.
    """
    errors: list[str] = []
    for expected_file in expected_dir.rglob("*"):
        if not expected_file.is_file():
            continue
        rel = expected_file.relative_to(expected_dir)
        actual_file = workspace / rel
        if not actual_file.exists():
            errors.append(f"Missing file: {rel}")
            continue
        # Normalize and compare
        expected_text = _normalize(expected_file.read_text())
        actual_text = _normalize(actual_file.read_text())
        if expected_text != actual_text:
            errors.append(f"Content mismatch: {rel}")
    if errors:
        return False, "; ".join(errors)
    return True, ""


def _normalize(text: str) -> str:
    """Normalize text for comparison: CRLF->LF, strip trailing whitespace, collapse blank lines."""
    text = text.replace("\r\n", "\n")
    lines = [line.rstrip() for line in text.split("\n")]
    # Collapse 3+ consecutive blank lines to 2
    result: list[str] = []
    blank_count = 0
    for line in lines:
        if line == "":
            blank_count += 1
            if blank_count <= 2:
                result.append(line)
        else:
            blank_count = 0
            result.append(line)
    return "\n".join(result).strip() + "\n"


# ---------------------------------------------------------------------------
# Task runner
# ---------------------------------------------------------------------------


def run_task(task: dict, variant: Variant, run_index: int, config: BenchConfig) -> TaskResult:
    """Run a single task with the given variant and return the result."""
    task_id = task["id"]
    start_ms = time.monotonic_ns() // 1_000_000

    # Copy input to temp workspace
    with tempfile.TemporaryDirectory(prefix=f"tau-bench-{task_id}-") as tmpdir:
        workspace = Path(tmpdir)
        # Copy input files to workspace root
        shutil.copytree(task["input_dir"], workspace, dirs_exist_ok=True)

        prompt = task["prompt"]
        error: str | None = None
        success = False
        session_result: SessionResult | None = None

        try:
            tools = variant.tools if variant.tools else None
            with TauSession(
                model=config.model,
                cwd=workspace,
                tools=tools,
                edit_mode=variant.edit_mode or config.edit_mode,
                timeout=config.timeout,
            ) as session:
                session_result = session.send(prompt)

            # Verify output
            success, verify_error = verify_output(workspace, task["expected_dir"])
            if not success:
                error = verify_error

        except Exception as exc:
            error = str(exc)
            success = False

        wall_clock_ms = (time.monotonic_ns() // 1_000_000) - start_ms
        cycles = count_cycles(session_result) if session_result else 0

        return TaskResult(
            task_id=task_id,
            variant=variant.name,
            run_index=run_index,
            success=success,
            wall_clock_ms=wall_clock_ms,
            input_tokens=session_result.input_tokens if session_result else 0,
            output_tokens=session_result.output_tokens if session_result else 0,
            turns=cycles,  # use cycle count as the turns metric
            tool_calls=session_result.tool_calls if session_result else 0,
            error=error,
            metadata={
                "language": task["language"],
                "cycle_count": cycles,
            },
        )


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Post-edit diagnostics benchmark runner",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("fixtures_dir", type=Path, help="Path to fixtures directory")
    parser.add_argument(
        "--variants",
        type=str,
        default=None,
        help="Comma-separated variant names to run (default: all)",
    )
    parser.add_argument(
        "--filter",
        type=str,
        default=None,
        help="Filter tasks by language, e.g. 'language=rust'",
    )
    BenchConfig.add_cli_args(parser)
    return parser


def parse_filter(filter_str: str | None) -> dict[str, str] | None:
    """Parse a filter string like 'language=rust' into a dict."""
    if not filter_str:
        return None
    parts = filter_str.split("=", 1)
    if len(parts) != 2:
        return None
    return {parts[0].strip(): parts[1].strip()}


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()
    config = BenchConfig.from_cli(args)

    fixtures_dir: Path = args.fixtures_dir
    if not fixtures_dir.is_dir():
        print(f"Error: fixtures directory not found: {fixtures_dir}", file=sys.stderr)
        sys.exit(1)

    # Discover tasks
    tasks = discover_tasks(fixtures_dir)
    if not tasks:
        print(f"Error: no tasks found in {fixtures_dir}", file=sys.stderr)
        sys.exit(1)

    # Apply filter
    task_filter = parse_filter(args.filter)
    if task_filter:
        tasks = [t for t in tasks if all(t.get(k) == v for k, v in task_filter.items())]
        if not tasks:
            print(f"Error: no tasks match filter {args.filter}", file=sys.stderr)
            sys.exit(1)

    # Get variants
    variant_names = args.variants.split(",") if args.variants else None
    variants = get_variants(variant_names)

    print(f"Running {BENCHMARK_NAME}")
    print(f"  Tasks: {len(tasks)}")
    print(f"  Variants: {[v.name for v in variants]}")
    print(f"  Runs per task: {config.runs_per_task}")
    print(f"  Total runs: {len(tasks) * len(variants) * config.runs_per_task}")
    print()

    # Run all combinations
    results: list[TaskResult] = []
    total = len(tasks) * len(variants) * config.runs_per_task
    completed = 0

    for variant in variants:
        for task in tasks:
            for run_idx in range(config.runs_per_task):
                completed += 1
                label = f"[{completed}/{total}] {variant.name} / {task['id']} (run {run_idx + 1})"
                print(f"  {label} ...", end=" ", flush=True)

                result = run_task(task, variant, run_idx, config)
                results.append(result)

                status = "PASS" if result.success else f"FAIL: {result.error}"
                print(f"{status} ({result.wall_clock_ms}ms, {result.metadata.get('cycle_count', '?')} cycles)")

    # Generate report
    print()
    reporter = Reporter(BENCHMARK_NAME, results, config)
    output_dir = config.output_dir
    output_dir.mkdir(parents=True, exist_ok=True)
    reporter.write(output_dir)
    print(f"Report written to {output_dir}")

    # Save to store
    try:
        store = ResultStore(BENCHMARK_NAME)
        report = reporter.json()
        run_id = store.save(report)
        print(f"Saved as run {run_id}")
    except Exception as exc:
        print(f"Warning: could not save to store: {exc}", file=sys.stderr)


if __name__ == "__main__":
    main()
