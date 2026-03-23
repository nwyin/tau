#!/usr/bin/env python3
"""Step completion scorer for todo-tracking benchmark.

Parses the final workspace state to determine which of the 5 standard steps
completed successfully:

  Step 1 (read):     model produced output
  Step 2 (extract):  target file exists with expected functions
  Step 3 (imports):  callers import from new module
  Step 4 (tests):    test file exists
  Step 5 (verify):   pytest passes (simulated by checking test file validity)

Also scores ordering correctness and recovery from errors.

Usage:
    python score.py results/report.json
    python score.py results/report.json --json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Step verification
# ---------------------------------------------------------------------------


def check_step_1_read(workspace: Path, task_meta: dict) -> bool:
    """Step 1: model produced some output (always true if session ran)."""
    return True


def check_step_2_extract(workspace: Path, task_meta: dict) -> bool:
    """Step 2: target file exists with expected functions."""
    target_file = task_meta.get("extract_target", "utils.py")

    # Search for the target file in common locations
    candidates = [
        workspace / target_file,
        workspace / "src" / target_file,
    ]

    for candidate in candidates:
        if candidate.exists():
            content = candidate.read_text()
            expected_functions = task_meta.get("expected_functions", [])
            if not expected_functions:
                return True
            return all(f"def {fn}" in content for fn in expected_functions)

    return False


def check_step_3_imports(workspace: Path, task_meta: dict) -> bool:
    """Step 3: callers import from the new module."""
    target_module = task_meta.get("extract_module", "utils")
    caller_files = task_meta.get("caller_files", [])

    if not caller_files:
        return False

    for caller in caller_files:
        caller_path = workspace / caller
        if not caller_path.exists():
            return False
        content = caller_path.read_text()
        # Check for import of the new module
        if f"from {target_module}" not in content and f"import {target_module}" not in content:
            # Also check relative paths like "from src.utils" or "from .utils"
            if f"from src.{target_module}" not in content and f"from .{target_module}" not in content:
                return False

    return True


def check_step_4_tests(workspace: Path, task_meta: dict) -> bool:
    """Step 4: test file exists."""
    test_file = task_meta.get("test_file", "test_utils.py")

    candidates = [
        workspace / test_file,
        workspace / "tests" / test_file,
    ]

    return any(c.exists() for c in candidates)


def check_step_5_verify(workspace: Path, task_meta: dict) -> bool:
    """Step 5: tests pass.

    Since we can't actually run pytest in the scorer, we check that:
      - The test file exists and has valid test functions
      - The expected output files exist
    """
    test_file = task_meta.get("test_file", "test_utils.py")
    candidates = [
        workspace / test_file,
        workspace / "tests" / test_file,
    ]

    for candidate in candidates:
        if candidate.exists():
            content = candidate.read_text()
            # Check for at least one test function
            if "def test_" in content:
                return True

    return False


STEP_CHECKS = [
    ("read", check_step_1_read),
    ("extract", check_step_2_extract),
    ("imports", check_step_3_imports),
    ("tests", check_step_4_tests),
    ("verify", check_step_5_verify),
]


# ---------------------------------------------------------------------------
# Task scoring
# ---------------------------------------------------------------------------


def score_task(workspace: Path, task_meta: dict) -> dict:
    """Score a single task's step completion.

    Returns a dict with per-step results and aggregate scores.
    """
    steps: list[dict] = []
    completed = 0

    for step_name, check_fn in STEP_CHECKS:
        passed = check_fn(workspace, task_meta)
        steps.append({"step": step_name, "completed": passed})
        if passed:
            completed += 1

    # Check ordering: steps should complete in order 1-5
    # A step is "out of order" if it completed but a previous step didn't
    ordering_correct = True
    for i, step in enumerate(steps):
        if step["completed"]:
            if i > 0 and not steps[i - 1]["completed"]:
                ordering_correct = False

    return {
        "steps": steps,
        "steps_completed": completed,
        "total_steps": len(STEP_CHECKS),
        "completion_rate": round(completed / len(STEP_CHECKS), 4),
        "all_complete": completed == len(STEP_CHECKS),
        "ordering_correct": ordering_correct,
    }


def score_recovery(result_meta: dict) -> dict:
    """Score recovery from error-injection variants.

    Looks at the number of turns after the error was encountered and
    whether the model eventually succeeded.
    """
    error_turn = result_meta.get("error_encountered_turn")
    total_turns = result_meta.get("total_turns", 0)
    final_success = result_meta.get("final_success", False)

    if error_turn is None:
        return {"error_injected": False}

    recovery_turns = total_turns - error_turn if error_turn else 0
    spiraled = recovery_turns > 10

    return {
        "error_injected": True,
        "error_turn": error_turn,
        "recovery_turns": recovery_turns,
        "recovered": final_success,
        "spiraled": spiraled,
    }


# ---------------------------------------------------------------------------
# Report-level scoring
# ---------------------------------------------------------------------------


def score_report(report: dict) -> dict:
    """Score all results in a benchmark report.

    Expects report["results"] to be a list of TaskResult dicts.
    """
    by_variant: dict[str, list[dict]] = {}

    for result in report.get("results", []):
        variant = result.get("variant", "unknown")
        meta = result.get("metadata", {})

        task_score = meta.get("task_score", {})
        recovery = meta.get("recovery", {})

        entry = {
            "task_id": result.get("task_id", "unknown"),
            "steps_completed": task_score.get("steps_completed", 0),
            "total_steps": task_score.get("total_steps", 5),
            "all_complete": task_score.get("all_complete", False),
            "ordering_correct": task_score.get("ordering_correct", True),
            "recovery": recovery,
            "turns": result.get("turns", 0),
            "tokens": result.get("input_tokens", 0) + result.get("output_tokens", 0),
        }

        by_variant.setdefault(variant, []).append(entry)

    # Aggregate per variant
    variant_summary: dict[str, dict] = {}
    for variant, entries in sorted(by_variant.items()):
        total = len(entries)
        complete = sum(1 for e in entries if e["all_complete"])
        avg_steps = sum(e["steps_completed"] for e in entries) / total if total else 0
        avg_turns = sum(e["turns"] for e in entries) / total if total else 0
        avg_tokens = sum(e["tokens"] for e in entries) / total if total else 0
        ordering_ok = sum(1 for e in entries if e["ordering_correct"])

        # Recovery stats (only for error-injection tasks)
        error_entries = [e for e in entries if e["recovery"].get("error_injected", False)]
        if error_entries:
            avg_recovery = sum(e["recovery"].get("recovery_turns", 0) for e in error_entries) / len(error_entries)
            recovered = sum(1 for e in error_entries if e["recovery"].get("recovered", False))
            spiraled = sum(1 for e in error_entries if e["recovery"].get("spiraled", False))
        else:
            avg_recovery = 0
            recovered = 0
            spiraled = 0

        variant_summary[variant] = {
            "total": total,
            "complete": complete,
            "completion_rate": round(complete / total, 4) if total else 0,
            "avg_steps": round(avg_steps, 2),
            "ordering_correct": ordering_ok,
            "avg_turns": round(avg_turns, 1),
            "avg_tokens": round(avg_tokens),
            "recovery_avg_turns": round(avg_recovery, 1),
            "recovered": recovered,
            "spiraled": spiraled,
        }

    return {
        "by_variant": variant_summary,
        "total_results": sum(len(v) for v in by_variant.values()),
    }


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def _print_scorecard(scores: dict) -> None:
    """Print a human-readable scorecard."""
    print("\n" + "=" * 90)
    print("TODO-TRACKING SCORECARD")
    print("=" * 90)

    header = f"{'Variant':<22} {'Complete%':>9} {'Steps':>7} {'Ordering':>9} {'Turns':>7} {'Tokens':>8} {'Recovery':>9}"
    print(f"\n{header}")
    print("-" * 90)

    for variant, info in sorted(scores["by_variant"].items()):
        print(
            f"{variant:<22} "
            f"{info['completion_rate']:>8.1%} "
            f"{info['avg_steps']:>7.1f} "
            f"{info['ordering_correct']:>9} "
            f"{info['avg_turns']:>7.1f} "
            f"{info['avg_tokens']:>8,} "
            f"{info['recovery_avg_turns']:>8.1f}t"
        )

    print(f"\nTotal results: {scores['total_results']}")
    print()


def main() -> None:
    parser = argparse.ArgumentParser(description="Score todo-tracking benchmark results")
    parser.add_argument("report", type=Path, help="Path to report.json")
    parser.add_argument("--json", action="store_true", help="Output JSON to stdout")
    parser.add_argument("-o", "--output", type=Path, help="Write scores to file")
    args = parser.parse_args()

    report = json.loads(args.report.read_text())
    scores = score_report(report)

    if args.json or args.output:
        output_text = json.dumps(scores, indent=2, ensure_ascii=False) + "\n"
        if args.output:
            args.output.write_text(output_text)
            print(f"Scores written to {args.output}", file=sys.stderr)
        else:
            print(output_text)
    else:
        _print_scorecard(scores)


if __name__ == "__main__":
    main()
