"""Runner for fuzzy-e2e benchmark.

Loads fixtures, runs each task under multiple edit strategy variants,
verifies output against expected files, and generates a report.

Adapted from edit-bench's runner pattern with variant iteration and
shared session infrastructure.

Usage:
    python run.py fixtures/ --model claude-sonnet-4-6 \\
        --variants tau-exact,tau-trimws,tau-fuzzy-92,tau-hashline,baseline-opi \\
        --timeout 180 --concurrency 4 -o results/

    python run.py fixtures/ --model claude-sonnet-4-6 \\
        --variants tau-exact --filter "difficulty=easy" -o results/debug/
"""

from __future__ import annotations

import argparse
import json
import shutil
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.config import BenchConfig
from shared.reporter import Reporter
from shared.result import TaskResult
from shared.session import TauSession
from shared.store import ResultStore
from shared.variants import Variant

from variants import get_variants
from verify import verify


# ── Prompt template ────────────────────────────────────────────────────

PROMPT_TEMPLATE = """\
You are working in a repository with a single edit task.

{task_prompt}

## Important constraints
- Make the minimum change necessary. Do not refactor, improve, or "clean up" other code.
- If you see multiple similar patterns, only change the ONE that is buggy (there is only one intended mutation).
- Preserve exact code structure. Do not rearrange statements or change formatting.
- Your output is verified by exact text diff against an expected fixture. "Equivalent" code, reordered imports, or formatting changes will fail.
- Prefer copying the original line(s) and changing only the specific token(s) required.
- After applying the fix, re-read the changed region to confirm you only touched the intended line(s).

Read the relevant files first, then use the edit tool to apply the fix.\
"""


# ── Task loading ───────────────────────────────────────────────────────


def load_tasks(fixtures_dir: Path, filter_expr: str | None = None) -> list[dict]:
    """Load all tasks from fixtures directory, optionally filtered."""
    tasks = []
    for task_dir in sorted(fixtures_dir.iterdir()):
        if not task_dir.is_dir():
            continue
        metadata_path = task_dir / "metadata.json"
        if not metadata_path.exists():
            continue
        metadata = json.loads(metadata_path.read_text())
        metadata["task_id"] = task_dir.name
        metadata["task_dir"] = str(task_dir)
        tasks.append(metadata)

    if filter_expr:
        tasks = _apply_filter(tasks, filter_expr)

    return tasks


def _apply_filter(tasks: list[dict], filter_expr: str) -> list[dict]:
    """Apply key=value filter to tasks based on metadata fields."""
    filtered = tasks
    for part in filter_expr.split(","):
        part = part.strip()
        if "=" not in part:
            continue
        key, value = part.split("=", 1)
        key = key.strip()
        value = value.strip()
        filtered = [t for t in filtered if str(t.get(key, "")).lower() == value.lower()]
    return filtered


# ── Single task execution ──────────────────────────────────────────────


def run_single_task(
    config: BenchConfig,
    task: dict,
    variant: Variant,
    run_index: int,
) -> TaskResult:
    """Run a single task: copy workspace, spawn session, send prompt, verify output."""
    task_id = task["task_id"]
    task_dir = Path(task["task_dir"])
    filename = task["file_name"]
    expected_path = task_dir / "expected" / filename

    with tempfile.TemporaryDirectory(prefix=f"fuzzy-e2e-{task_id}-") as tmp:
        tmp_path = Path(tmp)

        # Copy input file to workspace
        input_dir = task_dir / "input"
        for src_file in input_dir.iterdir():
            shutil.copy2(src_file, tmp_path / src_file.name)

        # Build prompt
        raw_prompt = (task_dir / "prompt.md").read_text()
        prompt = PROMPT_TEMPLATE.format(task_prompt=raw_prompt)

        # Determine edit mode from variant
        edit_mode = variant.edit_mode

        start = time.monotonic()
        total_input_tokens = 0
        total_output_tokens = 0
        total_tool_calls = 0
        turns = 0
        edit_successes = 0
        edit_attempts = 0
        retry_count = 0
        error_msg: str | None = None

        try:
            with TauSession(
                model=config.model,
                cwd=tmp_path,
                tools=["file_read", "file_edit", "file_write"],
                edit_mode=edit_mode,
                timeout=config.timeout,
            ) as session:
                # Send initial prompt
                result = session.send(prompt)
                turns += 1
                total_input_tokens += result.input_tokens
                total_output_tokens += result.output_tokens
                total_tool_calls += result.tool_calls

                # Check output
                actual_path = tmp_path / filename
                if not actual_path.exists():
                    error_msg = "output file missing after edit"
                else:
                    expected_text = expected_path.read_text()
                    actual_text = actual_path.read_text()
                    vr = verify(actual_text, expected_text, filename)

                    if vr.success:
                        elapsed_ms = int((time.monotonic() - start) * 1000)
                        return TaskResult(
                            task_id=task_id,
                            variant=variant.name,
                            run_index=run_index,
                            success=True,
                            wall_clock_ms=elapsed_ms,
                            input_tokens=total_input_tokens,
                            output_tokens=total_output_tokens,
                            turns=turns,
                            tool_calls=total_tool_calls,
                            metadata={
                                "edit_success_rate": 1.0,
                                "retry_count": 0,
                                "false_edit": False,
                            },
                        )
                    else:
                        error_msg = "verification failed"
                        # Check if the edit changed the wrong thing (false edit)
                        input_text = (task_dir / "input" / filename).read_text()
                        made_change = actual_text != input_text
                        if made_change and not vr.success:
                            # Changed something but not correctly -- possible false edit
                            pass

        except TimeoutError:
            error_msg = "timeout"
        except Exception as exc:
            error_msg = str(exc)

        elapsed_ms = int((time.monotonic() - start) * 1000)

        # Determine false edit status
        false_edit = False
        actual_path = tmp_path / filename
        if actual_path.exists():
            input_text = (task_dir / "input" / filename).read_text()
            actual_text = actual_path.read_text()
            expected_text = expected_path.read_text()
            made_change = actual_text != input_text
            vr = verify(actual_text, expected_text, filename)
            false_edit = made_change and not vr.success

        return TaskResult(
            task_id=task_id,
            variant=variant.name,
            run_index=run_index,
            success=False,
            wall_clock_ms=elapsed_ms,
            input_tokens=total_input_tokens,
            output_tokens=total_output_tokens,
            turns=turns,
            tool_calls=total_tool_calls,
            error=error_msg,
            metadata={
                "edit_success_rate": edit_successes / max(edit_attempts, 1),
                "retry_count": retry_count,
                "false_edit": false_edit,
            },
        )


# ── Main runner ────────────────────────────────────────────────────────


def run_benchmark(
    config: BenchConfig,
    tasks: list[dict],
    variants: list[Variant],
) -> list[TaskResult]:
    """Run all tasks x variants x runs, returning collected results."""
    work_items: list[tuple[dict, Variant, int]] = []
    for variant in variants:
        for task in tasks:
            for run_idx in range(config.runs_per_task):
                work_items.append((task, variant, run_idx))

    total = len(work_items)
    print(f"Running {len(tasks)} tasks x {len(variants)} variants x {config.runs_per_task} runs = {total} total")
    print(f"Model: {config.model}, Concurrency: {config.concurrency}")
    print()

    results: list[TaskResult] = []

    if config.concurrency <= 1:
        for i, (task, variant, run_idx) in enumerate(work_items, 1):
            task_id = task["task_id"]
            print(f"  [{i}/{total}] {task_id} ({variant.name}, run {run_idx + 1})...", end=" ", flush=True)
            result = run_single_task(config, task, variant, run_idx)
            status = "PASS" if result.success else "FAIL"
            tokens = result.input_tokens + result.output_tokens
            print(f"{status} ({result.wall_clock_ms}ms, {tokens} tokens)")
            results.append(result)
    else:
        print(f"Concurrency: {config.concurrency}")
        completed = 0
        with ThreadPoolExecutor(max_workers=config.concurrency) as executor:
            future_to_item = {}
            for task, variant, run_idx in work_items:
                future = executor.submit(run_single_task, config, task, variant, run_idx)
                future_to_item[future] = (task, variant, run_idx)

            for future in as_completed(future_to_item):
                task, variant, run_idx = future_to_item[future]
                task_id = task["task_id"]
                completed += 1
                try:
                    result = future.result()
                except Exception as exc:
                    result = TaskResult(
                        task_id=task_id,
                        variant=variant.name,
                        run_index=run_idx,
                        success=False,
                        wall_clock_ms=0,
                        input_tokens=0,
                        output_tokens=0,
                        turns=0,
                        tool_calls=0,
                        error=str(exc),
                    )
                status = "PASS" if result.success else "FAIL"
                tokens = result.input_tokens + result.output_tokens
                print(
                    f"  [{completed}/{total}] {task_id} ({variant.name}, run {run_idx + 1})... {status} ({result.wall_clock_ms}ms, {tokens} tokens)"
                )
                results.append(result)

    return results


# ── CLI ────────────────────────────────────────────────────────────────


def main() -> None:
    parser = argparse.ArgumentParser(description="Run fuzzy-e2e benchmark")
    parser.add_argument("fixtures", type=Path, help="Fixtures directory")
    parser.add_argument("--model", default="claude-sonnet-4-6", help="Model identifier")
    parser.add_argument("--variants", default=None, help="Comma-separated variant names (default: all)")
    parser.add_argument("--runs", type=int, default=1, help="Runs per task per variant (default: 1)")
    parser.add_argument("--timeout", type=int, default=180, help="Timeout per task in seconds (default: 180)")
    parser.add_argument("--concurrency", "-j", type=int, default=4, help="Parallel task execution (default: 4)")
    parser.add_argument("--filter", default=None, help="Filter tasks by metadata (e.g., difficulty=easy)")
    parser.add_argument("-o", "--output", type=Path, default=Path("results"), help="Output directory (default: results/)")
    parser.add_argument("--tau", default="tau", help="Path to tau binary (default: tau)")
    parser.add_argument("--json", action="store_true", help="Machine-readable JSON to stdout")
    args = parser.parse_args()

    # Parse variants
    variant_names = args.variants.split(",") if args.variants else None
    variants = get_variants(variant_names)

    config = BenchConfig(
        model=args.model,
        runs_per_task=args.runs,
        timeout=args.timeout,
        concurrency=args.concurrency,
        output_dir=args.output,
        tau_binary=args.tau,
    )

    # Load tasks
    tasks = load_tasks(args.fixtures, args.filter)
    if not tasks:
        print(f"No tasks found in {args.fixtures}")
        return

    print(f"Loaded {len(tasks)} tasks from {args.fixtures}")
    print(f"Variants: {', '.join(v.name for v in variants)}")

    # Run benchmark
    results = run_benchmark(config, tasks, variants)

    # Generate report
    reporter = Reporter(benchmark_name="fuzzy-e2e", results=results, config=config)
    args.output.mkdir(parents=True, exist_ok=True)
    reporter.write(args.output)

    # Store results
    store = ResultStore(benchmark="fuzzy-e2e")
    report_data = reporter.json_dict()
    run_id = store.save(report_data)
    print(f"\nResults saved: {run_id}")

    if args.json:
        print(reporter.json())
    else:
        print(reporter.markdown())

    # Print variant comparison summary
    by_variant = reporter.by_variant()
    print("\n## Variant Comparison")
    print(f"{'Variant':<20} {'Pass':>6} {'Total':>6} {'Rate':>8} {'Avg Tokens':>12} {'False Edit%':>12}")
    for vname, stats in by_variant.items():
        rate = stats.get("pass_rate", 0)
        avg_tok = stats.get("avg_tokens", 0)
        # Compute false edit rate from results
        v_results = [r for r in results if r.variant == vname]
        false_edits = sum(1 for r in v_results if r.metadata.get("false_edit", False))
        false_rate = false_edits / max(len(v_results), 1)
        print(f"{vname:<20} {stats.get('passed', 0):>6} {stats.get('total', 0):>6} {rate:>7.1%} {avg_tok:>12,.0f} {false_rate:>11.1%}")


if __name__ == "__main__":
    main()
