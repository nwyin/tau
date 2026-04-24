"""Parallel file operations benchmark runner.

Measures whether reading multiple files in parallel (single turn) saves
wall-clock time vs sequential reads. Spawns tau sessions for each
variant x file_count x run combination.

Usage:
    uv run python run.py fixtures/ --model claude-sonnet-4-6 --variants sequential,parallel,natural --runs 10 -o results/
"""

from __future__ import annotations

import argparse
import re
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

BENCHMARK_NAME = "parallel-ops"


# ---------------------------------------------------------------------------
# Fixture discovery
# ---------------------------------------------------------------------------


def discover_workspaces(fixtures_dir: Path, file_counts: list[int] | None = None) -> list[dict]:
    """Find workspace directories under fixtures_dir.

    Each workspace dir is named '{N}-files/' and contains src/ and prompt.md.
    Returns a list of dicts with keys: id, file_count, prompt, workspace_dir.
    """
    workspaces: list[dict] = []
    for entry in sorted(fixtures_dir.iterdir()):
        if not entry.is_dir():
            continue
        match = re.match(r"^(\d+)-files$", entry.name)
        if not match:
            continue
        count = int(match.group(1))
        if file_counts and count not in file_counts:
            continue
        prompt_file = entry / "prompt.md"
        if not prompt_file.exists():
            continue
        src_dir = entry / "src"
        if not src_dir.is_dir():
            continue
        actual_files = [f for f in src_dir.iterdir() if f.suffix == ".py"]
        workspaces.append(
            {
                "id": entry.name,
                "file_count": count,
                "prompt": prompt_file.read_text(),
                "workspace_dir": entry,
                "actual_file_count": len(actual_files),
            }
        )
    return workspaces


# ---------------------------------------------------------------------------
# Verification
# ---------------------------------------------------------------------------


def check_found_target(session_result: SessionResult) -> bool:
    """Check whether the model correctly identified process_data in search.py.

    Looks for mentions of both 'process_data' and 'search' in the output.
    """
    output_lower = session_result.output.lower()
    has_func = "process_data" in output_lower
    has_file = "search" in output_lower
    return has_func and has_file


# ---------------------------------------------------------------------------
# Task runner
# ---------------------------------------------------------------------------


def run_task(workspace: dict, variant: Variant, run_index: int, config: BenchConfig) -> TaskResult:
    """Run a single workspace with the given variant and return the result."""
    task_id = f"{workspace['id']}-{variant.name}"
    start_ms = time.monotonic_ns() // 1_000_000

    with tempfile.TemporaryDirectory(prefix=f"tau-bench-{task_id}-") as tmpdir:
        work_dir = Path(tmpdir)
        shutil.copytree(workspace["workspace_dir"], work_dir, dirs_exist_ok=True)

        prompt = workspace["prompt"]
        error: str | None = None
        success = False
        session_result: SessionResult | None = None
        session_turns = 0

        try:
            tools = variant.tools if variant.tools else None
            with TauSession(
                model=config.model,
                cwd=work_dir,
                tools=tools,
                edit_mode=config.edit_mode,
                timeout=config.timeout,
            ) as session:
                session_result = session.send(prompt)
                session_turns = session.turns

            success = check_found_target(session_result)
            if not success:
                error = "Model did not identify process_data in search.py"

        except Exception as exc:
            error = str(exc)
            success = False

        wall_clock_ms = (time.monotonic_ns() // 1_000_000) - start_ms

        return TaskResult.from_session(
            task_id=workspace["id"],
            variant=variant.name,
            run_index=run_index,
            success=success,
            wall_clock_ms=wall_clock_ms,
            session_result=session_result,
            turns=session_turns,
            error=error,
            metadata={
                "file_count": workspace["file_count"],
            },
        )


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Parallel file operations benchmark runner",
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
        "--file-counts",
        type=str,
        default=None,
        help="Comma-separated file counts to test (default: all available)",
    )
    BenchConfig.add_cli_args(parser)
    return parser


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()
    config = BenchConfig.from_cli(args)

    fixtures_dir: Path = args.fixtures_dir
    if not fixtures_dir.is_dir():
        print(f"Error: fixtures directory not found: {fixtures_dir}", file=sys.stderr)
        sys.exit(1)

    # Parse file counts
    file_counts: list[int] | None = None
    if args.file_counts:
        file_counts = [int(x.strip()) for x in args.file_counts.split(",")]

    # Discover workspaces
    workspaces = discover_workspaces(fixtures_dir, file_counts)
    if not workspaces:
        print(f"Error: no workspaces found in {fixtures_dir}", file=sys.stderr)
        sys.exit(1)

    # Get variants
    variant_names = args.variants.split(",") if args.variants else None
    variants = get_variants(variant_names)

    print(f"Running {BENCHMARK_NAME}")
    print(f"  Workspaces: {[w['id'] for w in workspaces]}")
    print(f"  Variants: {[v.name for v in variants]}")
    print(f"  Runs per combination: {config.runs_per_task}")
    print(f"  Total runs: {len(workspaces) * len(variants) * config.runs_per_task}")
    print()

    # Run all combinations
    results: list[TaskResult] = []
    total = len(workspaces) * len(variants) * config.runs_per_task
    completed = 0

    for variant in variants:
        for workspace in workspaces:
            for run_idx in range(config.runs_per_task):
                completed += 1
                label = f"[{completed}/{total}] {variant.name} / {workspace['id']} (run {run_idx + 1})"
                print(f"  {label} ...", end=" ", flush=True)

                result = run_task(workspace, variant, run_idx, config)
                results.append(result)

                status = "PASS" if result.success else f"FAIL: {result.error}"
                print(f"{status} ({result.wall_clock_ms}ms, {result.tool_calls} tool calls)")

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
        run_id = store.save(reporter.json_dict())
        print(f"Saved as run {run_id}")
    except Exception as exc:
        print(f"Warning: could not save to store: {exc}", file=sys.stderr)


if __name__ == "__main__":
    main()
