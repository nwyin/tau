#!/usr/bin/env python3
"""Runner for todo-tracking benchmark.

Runs multi-step refactoring tasks under different plan/todo tracking
variants and measures step completion, ordering, and recovery from errors.

Supports plan-mode's two-phase execution and periodic-inject's turn-counting.

Usage:
    python run.py fixtures/ \\
        --model claude-sonnet-4-6 \\
        --variants baseline,optional-tool,mandatory-prompt,plan-mode,periodic-inject \\
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

from score import score_task
from variants import get_variants

# ---------------------------------------------------------------------------
# Task loading
# ---------------------------------------------------------------------------


def load_tasks(fixture_dir: Path) -> list[dict]:
    """Load task fixtures from directory."""
    tasks: list[dict] = []
    for task_dir in sorted(fixture_dir.iterdir()):
        if not task_dir.is_dir():
            continue

        prompt_path = task_dir / "prompt.md"
        prompt_error_path = task_dir / "prompt-error.md"
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
                "prompt_error": prompt_error_path.read_text().strip() if prompt_error_path.exists() else None,
                "input_dir": task_dir / "input",
                "expected_dir": task_dir / "expected",
                "metadata": metadata,
                "has_error_variant": prompt_error_path.exists(),
            }
        )

    return tasks


# ---------------------------------------------------------------------------
# Runner variants
# ---------------------------------------------------------------------------


def _run_standard(
    session: TauSession,
    prompt: str,
    variant: Variant,
) -> tuple[list[SessionResult], int]:
    """Standard execution: send prompt, let model work."""
    result = session.send(prompt)
    return [result], 1


def _run_plan_mode(
    session: TauSession,
    prompt: str,
    variant: Variant,
) -> tuple[list[SessionResult], int]:
    """Plan-mode: two-phase execution.

    Phase 1: restricted tools (read-only), model produces a plan.
    Phase 2: full tools, plan re-injected as context.
    """
    # TODO: requires todo tracking feature in tau
    # Phase 1: send prompt with restricted tool set
    # The session should be configured with only read-only tools:
    #   session.configure(tools=["file_read", "grep", "glob"])
    plan_prompt = f"{prompt}\n\nFirst, explore the codebase and create a step-by-step plan. Do NOT make any changes yet."
    plan_result = session.send(plan_prompt)

    # Phase 2: re-inject plan and enable full tools
    # TODO: requires todo tracking feature in tau
    # session.configure(tools=None)  # restore all tools
    execute_prompt = f"Good plan. Now execute it step by step. Here's your plan for reference:\n\n{plan_result.output}"
    exec_result = session.send(execute_prompt)

    return [plan_result, exec_result], 2


def _run_periodic_inject(
    session: TauSession,
    prompt: str,
    variant: Variant,
    *,
    inject_interval: int = 10,
) -> tuple[list[SessionResult], int]:
    """Periodic-inject: re-inject plan state every N turns.

    For this benchmark, we simulate periodic injection by:
    1. Sending the initial prompt
    2. Checking turn count
    3. Injecting plan state reminder when interval is reached
    """
    # TODO: requires todo tracking feature in tau
    # In the full implementation, the session would:
    #   - Track turns automatically
    #   - Read .tau-plan.json every inject_interval turns
    #   - Inject "[Reminder] Current plan state: {plan_json}" as a system message
    result = session.send(prompt)
    return [result], 1


# ---------------------------------------------------------------------------
# Single task runner
# ---------------------------------------------------------------------------


def run_single(
    task: dict,
    variant: Variant,
    config: BenchConfig,
    run_index: int,
    *,
    use_error_prompt: bool = False,
) -> TaskResult:
    """Run a single task with a specific variant."""
    task_id = task["id"]
    if use_error_prompt:
        task_id = f"{task_id}-error"

    prompt = task["prompt_error"] if use_error_prompt and task["prompt_error"] else task["prompt"]

    start_time = time.monotonic()
    total_input_tokens = 0
    total_output_tokens = 0
    total_tool_calls = 0
    turn_count = 0

    with tempfile.TemporaryDirectory(prefix=f"bench-{task_id}-") as tmp:
        work_dir = Path(tmp)

        # Copy input files
        input_dir = task["input_dir"]
        if input_dir.exists():
            shutil.copytree(input_dir, work_dir, dirs_exist_ok=True)

        try:
            # TODO: requires compaction/todo feature in tau
            # Session configuration depends on variant:
            #   - baseline: standard config
            #   - optional-tool: add todo_write/todo_read tools
            #   - mandatory-prompt: add tools + system prompt suffix
            #   - plan-mode: two-phase with restricted tools
            #   - periodic-inject: add tools + turn-based injection
            with TauSession(
                model=config.model,
                cwd=work_dir,
                tools=variant.tools,
                timeout=config.timeout,
            ) as session:
                # Dispatch to variant-specific runner
                if variant.name == "plan-mode":
                    results, turns = _run_plan_mode(session, prompt, variant)
                elif variant.name == "periodic-inject":
                    interval = variant.tau_config_overrides.get("inject_interval", 10)
                    results, turns = _run_periodic_inject(session, prompt, variant, inject_interval=interval)
                else:
                    results, turns = _run_standard(session, prompt, variant)

                for r in results:
                    total_input_tokens += r.input_tokens
                    total_output_tokens += r.output_tokens
                    total_tool_calls += r.tool_calls
                turn_count = turns

            # Score step completion
            task_score = score_task(work_dir, task["metadata"])

            # Recovery scoring for error-injection variants
            recovery = {}
            if use_error_prompt:
                recovery = {
                    "error_injected": True,
                    "error_encountered_turn": task["metadata"].get("error_turn"),
                    "total_turns": turn_count,
                    "final_success": task_score["all_complete"],
                }

            elapsed_ms = int((time.monotonic() - start_time) * 1000)

            return TaskResult(
                task_id=task_id,
                variant=variant.name,
                run_index=run_index,
                success=task_score["all_complete"],
                wall_clock_ms=elapsed_ms,
                input_tokens=total_input_tokens,
                output_tokens=total_output_tokens,
                turns=turn_count,
                tool_calls=total_tool_calls,
                metadata={
                    "task_score": task_score,
                    "recovery": recovery,
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
                metadata={},
            )


# ---------------------------------------------------------------------------
# Main runner
# ---------------------------------------------------------------------------


def run_benchmark(
    fixture_dir: Path,
    variants: list[Variant],
    config: BenchConfig,
) -> list[TaskResult]:
    """Run all tasks x variants x runs (normal + error variants)."""
    tasks = load_tasks(fixture_dir)
    if not tasks:
        print(f"No tasks found in {fixture_dir}", file=sys.stderr)
        return []

    results: list[TaskResult] = []

    # Build work items: each (task, use_error, variant, run_idx)
    work_items: list[tuple[dict, bool, Variant, int]] = []
    for variant in variants:
        for task in tasks:
            for run_idx in range(config.runs_per_task):
                # Normal variant
                work_items.append((task, False, variant, run_idx))
                # Error-injection variant (if available)
                if task["has_error_variant"]:
                    work_items.append((task, True, variant, run_idx))

    total = len(work_items)
    for i, (task, use_error, variant, run_idx) in enumerate(work_items, 1):
        label = f"{task['id']}" + ("-error" if use_error else "")
        print(f"[{i}/{total}] {label} / {variant.name} / run {run_idx + 1}", file=sys.stderr)

        result = run_single(task, variant, config, run_idx, use_error_prompt=use_error)
        results.append(result)

        score = result.metadata.get("task_score", {})
        steps = score.get("steps_completed", "?")
        status = "PASS" if result.success else "FAIL"
        print(
            f"  -> {status} steps={steps}/5 tokens={result.input_tokens + result.output_tokens}",
            file=sys.stderr,
        )

    return results


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main() -> None:
    parser = argparse.ArgumentParser(description="Run todo-tracking benchmark")
    parser.add_argument("fixtures", type=Path, help="Path to fixtures directory")
    parser.add_argument("--model", default="claude-sonnet-4-6", help="Model to use")
    parser.add_argument("--variants", type=str, default=None, help="Comma-separated variant names (default: all)")
    parser.add_argument("--runs", type=int, default=3, help="Runs per task per variant (default: 3)")
    parser.add_argument("--timeout", type=int, default=120, help="Timeout per task in seconds")
    parser.add_argument("-o", "--output", type=Path, default=Path("results"), help="Output directory")
    parser.add_argument("--json", action="store_true", help="Output JSON to stdout")
    args = parser.parse_args()

    variant_names = args.variants.split(",") if args.variants else None
    variants = get_variants(variant_names)

    config = BenchConfig(
        model=args.model,
        runs_per_task=args.runs,
        timeout=args.timeout,
        output_dir=args.output,
    )

    results = run_benchmark(args.fixtures, variants, config)

    reporter = Reporter(benchmark_name="todo-tracking", results=results, config=config)

    if args.json:
        print(reporter.json())
    else:
        args.output.mkdir(parents=True, exist_ok=True)
        reporter.write(args.output)
        print(f"\nResults written to {args.output}/", file=sys.stderr)

        store = ResultStore(benchmark="todo-tracking")
        report = json.loads(reporter.json())
        run_id = store.save(report)
        print(f"Stored as run: {run_id}", file=sys.stderr)


if __name__ == "__main__":
    main()
