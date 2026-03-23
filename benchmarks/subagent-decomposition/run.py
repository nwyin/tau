"""Runner for subagent-decomposition benchmark.

Executes coordinated refactoring tasks under 4 variant strategies:
- single-agent:  one TauSession does everything
- sub-msg:       agent 1 extracts, summary passed to agents 2-N
- sub-discover:  agent 1 extracts, agents 2-N discover via file reads
- hive:          Hive orchestrator coordinates (placeholder)

Verification: runs pytest in workspace AND diffs against expected output.

Usage:
    python run.py fixtures/ --model claude-sonnet-4-6 \\
        --variants single-agent,sub-msg,sub-discover \\
        --runs 3 -o results/
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.config import BenchConfig
from shared.reporter import Reporter
from shared.result import TaskResult
from shared.session import SessionResult, TauSession
from shared.store import ResultStore
from shared.variants import Variant

from variants import get_variants


# ── Prompts ────────────────────────────────────────────────────────────

SINGLE_AGENT_PROMPT = """\
You are working in a codebase that needs refactoring.

{task_prompt}

Read all source files first to understand the codebase, then perform the extraction and update all callers.
Verify your changes are consistent by re-reading the modified files.
"""

EXTRACTION_AGENT_PROMPT = """\
You are Agent 1 in a multi-step refactoring task. Your job is to create the shared utility module.

{task_prompt}

## Your specific role
- Read the source files to identify the common functions
- Create `src/utils.py` with the extracted canonical functions
- Do NOT modify the handler files -- other agents will update them

When done, output a summary of what you extracted and how the handlers should update their imports.
"""

CALLER_UPDATE_MSG_PROMPT = """\
You are updating a handler file to use shared utilities that were just extracted.

## Context from extraction agent
{extraction_summary}

## Your task
Update `src/{handler_file}` to:
1. Remove the inlined utility functions that were extracted to `src/utils.py`
2. Add `from utils import {function_names}` at the top (after existing imports)
3. Do not change any business logic

Read `src/utils.py` first to confirm what was extracted, then update the handler file.
"""

CALLER_UPDATE_DISCOVER_PROMPT = """\
You are updating a handler file to use shared utilities.

Another agent has already created `src/utils.py` with extracted common functions.

## Your task
1. Read `src/utils.py` to discover what functions were extracted
2. Read `src/{handler_file}` to find which of those functions are duplicated
3. Remove the inlined copies and add proper imports from utils
4. Do not change any business logic

Start by reading `src/utils.py`, then read and update the handler.
"""


# ── Task loading ───────────────────────────────────────────────────────


def load_tasks(fixtures_dir: Path) -> list[dict]:
    """Load fixture tasks from the fixtures directory."""
    tasks = []
    for level_dir in sorted(fixtures_dir.iterdir()):
        if not level_dir.is_dir():
            continue
        metadata_path = level_dir / "metadata.json"
        if not metadata_path.exists():
            continue
        metadata = json.loads(metadata_path.read_text())
        metadata["task_id"] = level_dir.name
        metadata["task_dir"] = str(level_dir)
        tasks.append(metadata)
    return tasks


# ── Verification ───────────────────────────────────────────────────────


def verify_workspace(workspace: Path, expected_dir: Path, task_meta: dict) -> dict:
    """Verify workspace output against expected state.

    Returns verification dict with: success, tests_pass, files_correct,
    callers_correct, diff details.
    """
    result: dict = {
        "success": False,
        "tests_pass": False,
        "files_correct": 0,
        "files_total": 0,
        "callers_correct": 0,
        "callers_total": task_meta.get("handlers", 0),
        "has_utils": False,
        "diff": None,
    }

    # Check if utils.py was created
    utils_path = workspace / "src" / "utils.py"
    result["has_utils"] = utils_path.exists()

    # Run pytest
    try:
        test_result = subprocess.run(
            [sys.executable, "-m", "pytest", "tests/", "-q", "--tb=short"],
            cwd=str(workspace),
            capture_output=True,
            text=True,
            timeout=30,
            env={**__import__("os").environ, "PYTHONPATH": str(workspace / "src")},
        )
        result["tests_pass"] = test_result.returncode == 0
        if not result["tests_pass"]:
            result["test_output"] = test_result.stdout[-500:] if test_result.stdout else test_result.stderr[-500:]
    except (subprocess.TimeoutExpired, FileNotFoundError):
        result["tests_pass"] = False

    # Diff against expected output
    if expected_dir.exists():
        src_expected = expected_dir / "src"
        src_actual = workspace / "src"

        correct = 0
        total = 0
        callers_correct = 0
        diffs: list[str] = []

        for expected_file in sorted(src_expected.glob("*.py")):
            if expected_file.name == "__init__.py":
                continue
            total += 1
            actual_file = src_actual / expected_file.name

            if not actual_file.exists():
                diffs.append(f"MISSING: {expected_file.name}")
                continue

            expected_text = _normalize(expected_file.read_text())
            actual_text = _normalize(actual_file.read_text())

            if expected_text == actual_text:
                correct += 1
                if expected_file.name.endswith("_handler.py"):
                    callers_correct += 1
            else:
                import difflib

                diff_lines = list(
                    difflib.unified_diff(
                        expected_text.splitlines(keepends=True),
                        actual_text.splitlines(keepends=True),
                        fromfile=f"expected/{expected_file.name}",
                        tofile=f"actual/{expected_file.name}",
                        n=2,
                    )
                )
                diff_text = "".join(diff_lines)[:500]
                diffs.append(diff_text)
                # Partial credit: if the handler at least imports from utils, count it
                if expected_file.name.endswith("_handler.py") and "from utils import" in actual_text:
                    callers_correct += 1

        result["files_correct"] = correct
        result["files_total"] = total
        result["callers_correct"] = callers_correct
        if diffs:
            result["diff"] = "\n---\n".join(diffs)[:2000]

    result["success"] = result["tests_pass"] and result.get("has_utils", False)
    return result


def _normalize(text: str) -> str:
    """Normalize text for comparison."""
    import re

    text = text.replace("\r\n", "\n").replace("\r", "\n")
    lines = [line.rstrip() for line in text.split("\n")]
    while lines and not lines[-1]:
        lines.pop()
    result = "\n".join(lines) + "\n"
    result = re.sub(r"\n{3,}", "\n\n", result)
    return result


# ── Variant execution strategies ───────────────────────────────────────


def _prepare_workspace(task_dir: Path, tmp_path: Path) -> None:
    """Copy input workspace to temp directory."""
    input_dir = task_dir / "input"
    for item in input_dir.iterdir():
        dest = tmp_path / item.name
        if item.is_dir():
            shutil.copytree(item, dest)
        else:
            shutil.copy2(item, dest)


def _count_file_reads(result: SessionResult) -> int:
    """Estimate file_read calls from tool call count (rough heuristic)."""
    # Without parsing actual tool calls, we estimate ~40% of tool calls are reads
    return max(1, int(result.tool_calls * 0.4))


def run_single_agent(config: BenchConfig, task: dict, run_index: int) -> TaskResult:
    """Single agent does everything in one session."""
    task_id = task["task_id"]
    task_dir = Path(task["task_dir"])
    expected_dir = task_dir / "expected"
    prompt_text = (task_dir / "prompt.md").read_text()

    with tempfile.TemporaryDirectory(prefix=f"subagent-{task_id}-") as tmp:
        tmp_path = Path(tmp)
        _prepare_workspace(task_dir, tmp_path)

        prompt = SINGLE_AGENT_PROMPT.format(task_prompt=prompt_text)

        start = time.monotonic()
        total_input = 0
        total_output = 0
        total_tool_calls = 0
        error_msg: str | None = None

        try:
            with TauSession(
                model=config.model,
                cwd=tmp_path,
                edit_mode="replace",
                timeout=config.timeout,
            ) as session:
                result = session.send(prompt)
                total_input += result.input_tokens
                total_output += result.output_tokens
                total_tool_calls += result.tool_calls
        except TimeoutError:
            error_msg = "timeout"
        except Exception as exc:
            error_msg = str(exc)

        elapsed_ms = int((time.monotonic() - start) * 1000)

        # Verify
        verification = verify_workspace(tmp_path, expected_dir, task)

        return TaskResult(
            task_id=task_id,
            variant="single-agent",
            run_index=run_index,
            success=verification["success"],
            wall_clock_ms=elapsed_ms,
            input_tokens=total_input,
            output_tokens=total_output,
            turns=1,
            tool_calls=total_tool_calls,
            error=error_msg,
            metadata={
                "tests_pass": verification["tests_pass"],
                "files_correct": verification["files_correct"],
                "files_total": verification["files_total"],
                "callers_correct": verification["callers_correct"],
                "callers_total": verification["callers_total"],
                "has_utils": verification["has_utils"],
                "rework_rate": 0.0,
                "coordination_failure": False,
            },
        )


def run_sub_msg(config: BenchConfig, task: dict, run_index: int) -> TaskResult:
    """Agent 1 extracts, captures output, passes summary to agents 2-N."""
    task_id = task["task_id"]
    task_dir = Path(task["task_dir"])
    expected_dir = task_dir / "expected"
    prompt_text = (task_dir / "prompt.md").read_text()
    handler_count = task.get("handlers", 3)
    fn_names = task.get("function_names", ["parse_header"])

    with tempfile.TemporaryDirectory(prefix=f"subagent-msg-{task_id}-") as tmp:
        tmp_path = Path(tmp)
        _prepare_workspace(task_dir, tmp_path)

        start = time.monotonic()
        total_input = 0
        total_output = 0
        total_tool_calls = 0
        total_file_reads = 0
        turns = 0
        error_msg: str | None = None
        coordination_failure = False

        # Phase 1: extraction agent
        extraction_prompt = EXTRACTION_AGENT_PROMPT.format(task_prompt=prompt_text)
        extraction_summary = ""

        try:
            with TauSession(
                model=config.model,
                cwd=tmp_path,
                edit_mode="replace",
                timeout=config.timeout,
            ) as session:
                result = session.send(extraction_prompt)
                total_input += result.input_tokens
                total_output += result.output_tokens
                total_tool_calls += result.tool_calls
                total_file_reads += _count_file_reads(result)
                turns += 1
                extraction_summary = result.output
        except TimeoutError:
            error_msg = "timeout during extraction"
        except Exception as exc:
            error_msg = f"extraction failed: {exc}"

        if not error_msg:
            # Phase 2: caller update agents (in parallel)
            handler_files = [f for f in (tmp_path / "src").glob("*_handler.py")]
            fn_names_str = ", ".join(fn_names)

            def update_handler(handler_path: Path) -> SessionResult | str:
                handler_name = handler_path.name
                prompt = CALLER_UPDATE_MSG_PROMPT.format(
                    extraction_summary=extraction_summary,
                    handler_file=handler_name,
                    function_names=fn_names_str,
                )
                try:
                    with TauSession(
                        model=config.model,
                        cwd=tmp_path,
                        edit_mode="replace",
                        timeout=config.timeout,
                    ) as sess:
                        return sess.send(prompt)
                except Exception as exc:
                    return str(exc)

            with ThreadPoolExecutor(max_workers=min(len(handler_files), 4)) as executor:
                futures = {executor.submit(update_handler, hf): hf for hf in handler_files}
                for future in as_completed(futures):
                    handler_path = futures[future]
                    try:
                        res = future.result()
                        if isinstance(res, SessionResult):
                            total_input += res.input_tokens
                            total_output += res.output_tokens
                            total_tool_calls += res.tool_calls
                            total_file_reads += _count_file_reads(res)
                            turns += 1
                        else:
                            coordination_failure = True
                            if not error_msg:
                                error_msg = f"caller update failed for {handler_path.name}: {res}"
                    except Exception as exc:
                        coordination_failure = True
                        if not error_msg:
                            error_msg = f"caller update error: {exc}"

        elapsed_ms = int((time.monotonic() - start) * 1000)

        # Verify
        verification = verify_workspace(tmp_path, expected_dir, task)

        # Estimate rework rate: file reads beyond minimum (each handler read once = baseline)
        min_reads = handler_count + 1  # each handler + utils.py
        rework_rate = max(0, (total_file_reads - min_reads) / max(total_file_reads, 1))

        return TaskResult(
            task_id=task_id,
            variant="sub-msg",
            run_index=run_index,
            success=verification["success"],
            wall_clock_ms=elapsed_ms,
            input_tokens=total_input,
            output_tokens=total_output,
            turns=turns,
            tool_calls=total_tool_calls,
            error=error_msg,
            metadata={
                "tests_pass": verification["tests_pass"],
                "files_correct": verification["files_correct"],
                "files_total": verification["files_total"],
                "callers_correct": verification["callers_correct"],
                "callers_total": verification["callers_total"],
                "has_utils": verification["has_utils"],
                "rework_rate": rework_rate,
                "coordination_failure": coordination_failure,
            },
        )


def run_sub_discover(config: BenchConfig, task: dict, run_index: int) -> TaskResult:
    """Agent 1 extracts, agents 2-N discover changes by reading files (no context passing)."""
    task_id = task["task_id"]
    task_dir = Path(task["task_dir"])
    expected_dir = task_dir / "expected"
    prompt_text = (task_dir / "prompt.md").read_text()
    handler_count = task.get("handlers", 3)

    with tempfile.TemporaryDirectory(prefix=f"subagent-disc-{task_id}-") as tmp:
        tmp_path = Path(tmp)
        _prepare_workspace(task_dir, tmp_path)

        start = time.monotonic()
        total_input = 0
        total_output = 0
        total_tool_calls = 0
        total_file_reads = 0
        turns = 0
        error_msg: str | None = None
        coordination_failure = False

        # Phase 1: extraction agent
        extraction_prompt = EXTRACTION_AGENT_PROMPT.format(task_prompt=prompt_text)

        try:
            with TauSession(
                model=config.model,
                cwd=tmp_path,
                edit_mode="replace",
                timeout=config.timeout,
            ) as session:
                result = session.send(extraction_prompt)
                total_input += result.input_tokens
                total_output += result.output_tokens
                total_tool_calls += result.tool_calls
                total_file_reads += _count_file_reads(result)
                turns += 1
        except TimeoutError:
            error_msg = "timeout during extraction"
        except Exception as exc:
            error_msg = f"extraction failed: {exc}"

        if not error_msg:
            # Phase 2: caller update agents (in parallel, NO context from agent 1)
            handler_files = [f for f in (tmp_path / "src").glob("*_handler.py")]

            def update_handler_discover(handler_path: Path) -> SessionResult | str:
                handler_name = handler_path.name
                prompt = CALLER_UPDATE_DISCOVER_PROMPT.format(handler_file=handler_name)
                try:
                    with TauSession(
                        model=config.model,
                        cwd=tmp_path,
                        edit_mode="replace",
                        timeout=config.timeout,
                    ) as sess:
                        return sess.send(prompt)
                except Exception as exc:
                    return str(exc)

            with ThreadPoolExecutor(max_workers=min(len(handler_files), 4)) as executor:
                futures = {executor.submit(update_handler_discover, hf): hf for hf in handler_files}
                for future in as_completed(futures):
                    handler_path = futures[future]
                    try:
                        res = future.result()
                        if isinstance(res, SessionResult):
                            total_input += res.input_tokens
                            total_output += res.output_tokens
                            total_tool_calls += res.tool_calls
                            total_file_reads += _count_file_reads(res)
                            turns += 1
                        else:
                            coordination_failure = True
                            if not error_msg:
                                error_msg = f"caller update failed for {handler_path.name}: {res}"
                    except Exception as exc:
                        coordination_failure = True
                        if not error_msg:
                            error_msg = f"caller update error: {exc}"

        elapsed_ms = int((time.monotonic() - start) * 1000)

        verification = verify_workspace(tmp_path, expected_dir, task)

        # Discovery mode has higher rework: each agent reads utils.py + its handler + possibly others
        min_reads = handler_count + 1
        rework_rate = max(0, (total_file_reads - min_reads) / max(total_file_reads, 1))

        return TaskResult(
            task_id=task_id,
            variant="sub-discover",
            run_index=run_index,
            success=verification["success"],
            wall_clock_ms=elapsed_ms,
            input_tokens=total_input,
            output_tokens=total_output,
            turns=turns,
            tool_calls=total_tool_calls,
            error=error_msg,
            metadata={
                "tests_pass": verification["tests_pass"],
                "files_correct": verification["files_correct"],
                "files_total": verification["files_total"],
                "callers_correct": verification["callers_correct"],
                "callers_total": verification["callers_total"],
                "has_utils": verification["has_utils"],
                "rework_rate": rework_rate,
                "coordination_failure": coordination_failure,
            },
        )


def run_hive(config: BenchConfig, task: dict, run_index: int) -> TaskResult:
    """Hive orchestrator coordinates extraction + worker updates.

    TODO: Implement when Hive API is available. Currently returns a
    placeholder failure result.
    """
    task_id = task["task_id"]
    start = time.monotonic()
    elapsed_ms = int((time.monotonic() - start) * 1000)

    return TaskResult(
        task_id=task_id,
        variant="hive",
        run_index=run_index,
        success=False,
        wall_clock_ms=elapsed_ms,
        input_tokens=0,
        output_tokens=0,
        turns=0,
        tool_calls=0,
        error="hive variant not yet implemented",
        metadata={
            "tests_pass": False,
            "files_correct": 0,
            "files_total": 0,
            "callers_correct": 0,
            "callers_total": task.get("handlers", 0),
            "has_utils": False,
            "rework_rate": 0.0,
            "coordination_failure": False,
        },
    )


# Dispatch table
VARIANT_RUNNERS = {
    "single-agent": run_single_agent,
    "sub-msg": run_sub_msg,
    "sub-discover": run_sub_discover,
    "hive": run_hive,
}


# ── Main runner ────────────────────────────────────────────────────────


def run_benchmark(
    config: BenchConfig,
    tasks: list[dict],
    variants: list[Variant],
) -> list[TaskResult]:
    """Run all tasks x variants x runs."""
    work_items: list[tuple[dict, Variant, int]] = []
    for variant in variants:
        for task in tasks:
            for run_idx in range(config.runs_per_task):
                work_items.append((task, variant, run_idx))

    total = len(work_items)
    print(f"Running {len(tasks)} tasks x {len(variants)} variants x {config.runs_per_task} runs = {total} total")
    print(f"Model: {config.model}")
    print()

    results: list[TaskResult] = []

    for i, (task, variant, run_idx) in enumerate(work_items, 1):
        task_id = task["task_id"]
        runner = VARIANT_RUNNERS.get(variant.name)
        if not runner:
            print(f"  [{i}/{total}] SKIP {task_id} ({variant.name}) -- no runner")
            continue

        print(f"  [{i}/{total}] {task_id} ({variant.name}, run {run_idx + 1})...", end=" ", flush=True)
        result = runner(config, task, run_idx)
        status = "PASS" if result.success else "FAIL"
        tokens = result.input_tokens + result.output_tokens
        callers = f"{result.metadata.get('callers_correct', 0)}/{result.metadata.get('callers_total', 0)}"
        print(f"{status} (callers: {callers}, {result.wall_clock_ms}ms, {tokens} tokens)")
        results.append(result)

    return results


# ── CLI ────────────────────────────────────────────────────────────────


def main() -> None:
    parser = argparse.ArgumentParser(description="Run subagent-decomposition benchmark")
    parser.add_argument("fixtures", type=Path, help="Fixtures directory")
    parser.add_argument("--model", default="claude-sonnet-4-6", help="Model identifier")
    parser.add_argument("--variants", default=None, help="Comma-separated variant names (default: all)")
    parser.add_argument("--runs", type=int, default=3, help="Runs per task per variant (default: 3)")
    parser.add_argument("--timeout", type=int, default=300, help="Timeout per task in seconds (default: 300)")
    parser.add_argument("-o", "--output", type=Path, default=Path("results"), help="Output directory")
    parser.add_argument("--tau", default="tau", help="Path to tau binary")
    parser.add_argument("--json", action="store_true", help="Machine-readable JSON to stdout")
    args = parser.parse_args()

    variant_names = args.variants.split(",") if args.variants else None
    variants = get_variants(variant_names)

    config = BenchConfig(
        model=args.model,
        runs_per_task=args.runs,
        timeout=args.timeout,
        concurrency=1,  # sub-agent variants handle their own parallelism
        output_dir=args.output,
        tau_binary=args.tau,
    )

    tasks = load_tasks(args.fixtures)
    if not tasks:
        print(f"No tasks found in {args.fixtures}")
        return

    print(f"Loaded {len(tasks)} tasks from {args.fixtures}")
    print(f"Variants: {', '.join(v.name for v in variants)}")

    results = run_benchmark(config, tasks, variants)

    reporter = Reporter(benchmark_name="subagent-decomposition", results=results, config=config)
    args.output.mkdir(parents=True, exist_ok=True)
    reporter.write(args.output)

    store = ResultStore(benchmark="subagent-decomposition")
    report_data = reporter.json_dict()
    run_id = store.save(report_data)
    print(f"\nResults saved: {run_id}")

    if args.json:
        print(reporter.json())
    else:
        print(reporter.markdown())


if __name__ == "__main__":
    main()
