#!/usr/bin/env python3
"""Runner for compaction-efficiency benchmark.

Runs coding tasks under a matrix of (strategy x compression_level) configs,
measuring task success rate and token usage for each combination.

Usage:
    python run.py fixtures/ \\
        --model claude-sonnet-4-6 \\
        --strategies none,truncation,observation-mask,llm-summary,progressive \\
        --compression conservative,moderate,aggressive \\
        --runs 3 \\
        -o results/
"""

from __future__ import annotations

import argparse
import json
import shutil
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.config import BenchConfig
from shared.reporter import Reporter
from shared.result import TaskResult
from shared.session import SessionResult, TauSession
from shared.store import ResultStore
from shared.variants import Variant

from variants import ALL_COMPRESSIONS, ALL_STRATEGIES, build_variant_matrix

# ---------------------------------------------------------------------------
# Task loading
# ---------------------------------------------------------------------------


def load_tasks(fixture_dir: Path) -> list[dict]:
    """Load task fixtures from directory.

    Each task is a subdirectory with input/, expected/, and prompt.md.
    """
    tasks: list[dict] = []
    for task_dir in sorted(fixture_dir.iterdir()):
        if not task_dir.is_dir():
            continue

        prompt_path = task_dir / "prompt.md"
        if not prompt_path.exists():
            continue

        metadata_path = task_dir / "metadata.json"
        metadata = {}
        if metadata_path.exists():
            metadata = json.loads(metadata_path.read_text())

        tasks.append(
            {
                "id": task_dir.name,
                "dir": task_dir,
                "prompt": prompt_path.read_text().strip(),
                "input_dir": task_dir / "input",
                "expected_dir": task_dir / "expected",
                "complexity": metadata.get("complexity", "unknown"),
                "metadata": metadata,
            }
        )

    return tasks


# ---------------------------------------------------------------------------
# Task verification
# ---------------------------------------------------------------------------


def verify_task(work_dir: Path, expected_dir: Path) -> dict:
    """Verify task output against expected state.

    Simple file-by-file comparison.  Returns verification results.
    """
    if not expected_dir.exists():
        return {"verified": False, "reason": "no expected/ directory"}

    results: dict[str, str] = {}
    all_match = True

    for expected_file in sorted(expected_dir.rglob("*")):
        if not expected_file.is_file():
            continue

        rel = expected_file.relative_to(expected_dir)
        actual_file = work_dir / rel

        if not actual_file.exists():
            results[str(rel)] = "missing"
            all_match = False
            continue

        expected_content = expected_file.read_text().strip()
        actual_content = actual_file.read_text().strip()

        if expected_content == actual_content:
            results[str(rel)] = "match"
        else:
            results[str(rel)] = "mismatch"
            all_match = False

    return {
        "verified": True,
        "success": all_match,
        "files": results,
    }


# ---------------------------------------------------------------------------
# Single task runner
# ---------------------------------------------------------------------------


def run_single(
    task: dict,
    variant: Variant,
    config: BenchConfig,
    run_index: int,
) -> TaskResult:
    """Run a single task with a specific compaction variant."""
    task_id = task["id"]
    start_time = time.monotonic()

    # Create a temporary working directory with a copy of input files
    with tempfile.TemporaryDirectory(prefix=f"bench-{task_id}-") as tmp:
        work_dir = Path(tmp)

        # Copy input files into work dir
        input_dir = task["input_dir"]
        if input_dir.exists():
            shutil.copytree(input_dir, work_dir, dirs_exist_ok=True)

        total_input_tokens = 0
        total_output_tokens = 0
        total_tool_calls = 0
        turn_count = 0
        compaction_overhead_ms = 0
        tokens_before_compaction = 0
        tokens_after_compaction = 0

        try:
            # TODO: requires compaction feature in tau
            # The session needs to be configured with compaction settings
            # from variant.tau_config_overrides:
            #   - compaction_strategy
            #   - compaction_keep_ratio
            with TauSession(
                model=config.model,
                cwd=work_dir,
                timeout=config.timeout,
            ) as session:
                # Send the task prompt
                result: SessionResult = session.send(task["prompt"])
                total_input_tokens += result.input_tokens
                total_output_tokens += result.output_tokens
                total_tool_calls += result.tool_calls
                turn_count += 1

                # TODO: requires compaction feature in tau
                # The model will work through the task, potentially triggering
                # compaction when token usage crosses the threshold.
                # We need to capture:
                #   - tokens_before_compaction
                #   - tokens_after_compaction
                #   - compaction_overhead_ms (time for the compaction step)

            # Verify output
            verification = verify_task(work_dir, task["expected_dir"])
            success = verification.get("success", False)

            elapsed_ms = int((time.monotonic() - start_time) * 1000)

            compression_ratio = tokens_after_compaction / tokens_before_compaction if tokens_before_compaction > 0 else 1.0

            return TaskResult(
                task_id=task_id,
                variant=variant.name,
                run_index=run_index,
                success=success,
                wall_clock_ms=elapsed_ms,
                input_tokens=total_input_tokens,
                output_tokens=total_output_tokens,
                turns=turn_count,
                tool_calls=total_tool_calls,
                metadata={
                    "complexity": task["complexity"],
                    "compression_ratio": round(compression_ratio, 4),
                    "compaction_overhead_ms": compaction_overhead_ms,
                    "tokens_before_compaction": tokens_before_compaction,
                    "tokens_after_compaction": tokens_after_compaction,
                    "verification": verification,
                },
            )

        except Exception as e:
            elapsed_ms = int((time.monotonic() - start_time) * 1000)
            return TaskResult(
                task_id=task_id,
                variant=variant.name,
                run_index=run_index,
                success=False,
                wall_clock_ms=elapsed_ms,
                input_tokens=total_input_tokens,
                output_tokens=total_output_tokens,
                turns=turn_count,
                tool_calls=total_tool_calls,
                error=str(e),
                metadata={"complexity": task["complexity"]},
            )


# ---------------------------------------------------------------------------
# Main runner
# ---------------------------------------------------------------------------


def run_benchmark(
    fixture_dir: Path,
    variants: list[Variant],
    config: BenchConfig,
) -> list[TaskResult]:
    """Run all tasks x variants x runs."""
    tasks = load_tasks(fixture_dir)
    if not tasks:
        print(f"No tasks found in {fixture_dir}", file=sys.stderr)
        return []

    results: list[TaskResult] = []
    total = len(tasks) * len(variants) * config.runs_per_task
    completed = 0

    # For the "none" baseline, only run once regardless of runs_per_task
    # to save cost (full context is expensive)
    for variant in variants:
        runs = 1 if variant.name == "none" and config.runs_per_task > 1 else config.runs_per_task

        for task in tasks:
            for run_idx in range(runs):
                completed += 1
                print(
                    f"[{completed}/{total}] {task['id']} / {variant.name} / run {run_idx + 1}",
                    file=sys.stderr,
                )
                result = run_single(task, variant, config, run_idx)
                results.append(result)

                status = "PASS" if result.success else "FAIL"
                print(
                    f"  -> {status} tokens={result.input_tokens + result.output_tokens} time={result.wall_clock_ms}ms",
                    file=sys.stderr,
                )

    return results


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main() -> None:
    parser = argparse.ArgumentParser(description="Run compaction-efficiency benchmark")
    parser.add_argument("fixtures", type=Path, help="Path to fixtures directory")
    parser.add_argument("--model", default="claude-sonnet-4-6", help="Model to use")
    parser.add_argument(
        "--strategies",
        type=str,
        default=None,
        help=f"Comma-separated strategies (default: all). Options: {','.join(ALL_STRATEGIES)}",
    )
    parser.add_argument(
        "--compression",
        type=str,
        default=None,
        help=f"Comma-separated compression levels (default: all). Options: {','.join(ALL_COMPRESSIONS)}",
    )
    parser.add_argument("--runs", type=int, default=3, help="Runs per task per variant (default: 3)")
    parser.add_argument("--timeout", type=int, default=120, help="Timeout per task in seconds (default: 120)")
    parser.add_argument("--concurrency", type=int, default=2, help="Parallel tasks (default: 2, keep low for compaction)")
    parser.add_argument("-o", "--output", type=Path, default=Path("results"), help="Output directory")
    parser.add_argument("--json", action="store_true", help="Output JSON to stdout")
    args = parser.parse_args()

    strategies = args.strategies.split(",") if args.strategies else None
    compressions = args.compression.split(",") if args.compression else None
    variants = build_variant_matrix(strategies=strategies, compressions=compressions)

    print(f"Running {len(variants)} variant configs:", file=sys.stderr)
    for v in variants:
        print(f"  - {v.name}: {v.description}", file=sys.stderr)

    config = BenchConfig(
        model=args.model,
        runs_per_task=args.runs,
        timeout=args.timeout,
        output_dir=args.output,
        concurrency=args.concurrency,
    )

    results = run_benchmark(args.fixtures, variants, config)

    reporter = Reporter(benchmark_name="compaction-efficiency", results=results, config=config)

    if args.json:
        print(reporter.json())
    else:
        args.output.mkdir(parents=True, exist_ok=True)
        reporter.write(args.output)
        print(f"\nResults written to {args.output}/", file=sys.stderr)

        store = ResultStore(benchmark="compaction-efficiency")
        report = json.loads(reporter.json())
        run_id = store.save(report)
        print(f"Stored as run: {run_id}", file=sys.stderr)


if __name__ == "__main__":
    main()
