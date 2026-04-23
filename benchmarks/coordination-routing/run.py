#!/usr/bin/env python3
"""Runner for coordination-routing benchmark.

This benchmark measures whether a dependent critic thread actually consumes
upstream thread outputs under different orchestration shapes.

Usage:
    uv run python run.py fixtures/ \
        --model gpt-5.4-mini \
        --variants naive-parallel,prompt-only-parallel,staged-pipeline,document-polling \
        --runs 3 \
        -o results/
"""

from __future__ import annotations

import argparse
import json
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.config import BenchConfig
from shared.reporter import Reporter
from shared.result import TaskResult
from shared.session import SessionResult, TauSession
from shared.store import ResultStore
from shared.variants import Variant

from score import CoordinationExpectations, score_from_trace_file
from variants import get_variants

BENCHMARK_NAME = "coordination-routing"


def load_tasks(fixtures_dir: Path) -> list[dict[str, Any]]:
    """Load benchmark tasks from fixture directories."""
    tasks: list[dict[str, Any]] = []
    for task_dir in sorted(fixtures_dir.iterdir()):
        if not task_dir.is_dir():
            continue
        prompt_path = task_dir / "prompt.md"
        metadata_path = task_dir / "metadata.json"
        if not prompt_path.exists() or not metadata_path.exists():
            continue

        metadata = json.loads(metadata_path.read_text())
        expectations = CoordinationExpectations.from_metadata(metadata)
        tasks.append(
            {
                "id": task_dir.name,
                "dir": task_dir,
                "prompt": prompt_path.read_text().strip(),
                "metadata": metadata,
                "expectations": expectations,
            }
        )
    return tasks


def build_variant_prompt(task_prompt: str, variant_name: str, exp: CoordinationExpectations) -> str:
    """Build the benchmark prompt for a specific orchestration variant."""
    base = f"""{task_prompt}

Execution requirements (all variants):
- Use these exact thread aliases: `{exp.pro_alias}`, `{exp.con_alias}`, `{exp.critic_alias}`.
- `{exp.pro_alias}` must write to document `{exp.pro_doc}`.
- `{exp.con_alias}` must write to document `{exp.con_doc}`.
- Final response must include at least one `PRO_*` anchor and one `CON_*` anchor from the thread outputs.
- Avoid web tools; this is a coordination test.
"""

    if variant_name == "naive-parallel":
        mode = f"""
Variant mode: `naive-parallel`
- In one parallel batch, launch all three threads (`{exp.pro_alias}`, `{exp.con_alias}`, `{exp.critic_alias}`).
- Do not pass episodes to `{exp.critic_alias}`.
- After all threads finish, produce the final synthesis.
"""
    elif variant_name == "prompt-only-parallel":
        mode = f"""
Variant mode: `prompt-only-parallel`
- In one parallel batch, launch all three threads (`{exp.pro_alias}`, `{exp.con_alias}`, `{exp.critic_alias}`).
- Do not pass episodes to `{exp.critic_alias}`.
- In the critic task text, explicitly require waiting until both `{exp.pro_doc}` and `{exp.con_doc}` are available,
  and require document reads before critique.
- After all threads finish, produce the final synthesis.
"""
    elif variant_name == "staged-pipeline":
        mode = f"""
Variant mode: `staged-pipeline`
- Phase 1: launch `{exp.pro_alias}` and `{exp.con_alias}` in parallel.
- Phase 2: launch `{exp.critic_alias}` only after phase 1 completes.
- When launching `{exp.critic_alias}`, pass `episodes=["{exp.pro_alias}", "{exp.con_alias}"]`.
- Produce final synthesis after the critic finishes.
"""
    elif variant_name == "document-polling":
        mode = f"""
Variant mode: `document-polling`
- Launch all three threads in parallel.
- Critic instructions must explicitly require polling reads of `{exp.pro_doc}` and `{exp.con_doc}` until
  both contain anchor facts, and must forbid completion before both docs are read.
- Do not pass episodes to `{exp.critic_alias}`.
- Produce final synthesis after all threads finish.
"""
    else:
        mode = f"Variant mode: `{variant_name}`"

    return f"{base}\n{mode}".strip()


def run_task(task: dict[str, Any], variant: Variant, run_index: int, config: BenchConfig) -> TaskResult:
    """Run a single task/variant/run combination."""
    start = time.monotonic()
    prompt = build_variant_prompt(task["prompt"], variant.name, task["expectations"])

    session_result: SessionResult | None = None
    session_turns = 0
    error: str | None = None

    with tempfile.TemporaryDirectory(prefix=f"tau-bench-{BENCHMARK_NAME}-") as tmp_dir:
        work_dir = Path(tmp_dir)
        trace_dir = work_dir / "trace"
        trace_file = trace_dir / "trace.jsonl"
        task_id = f"{task['id']}-{variant.name}-run{run_index + 1}"

        try:
            with TauSession(
                model=config.model,
                cwd=work_dir,
                tools=variant.tools,
                edit_mode=variant.edit_mode or config.edit_mode,
                trace_output=trace_dir,
                task_id=task_id,
                timeout=config.timeout,
                tau_binary=config.tau_binary,
            ) as session:
                session_result = session.send(prompt)
                session_turns = session.turns
        except Exception as exc:
            error = str(exc)

        output_text = session_result.output if session_result else ""
        score = score_from_trace_file(
            trace_path=trace_file,
            output_text=output_text,
            variant_name=variant.name,
            expectations=task["expectations"],
        )

        success = bool(score["coordination_success"])
        if error is None and not success:
            error = score["success_reason"]
        if session_result and session_result.output.startswith("error:"):
            success = False
            error = session_result.output

        elapsed_ms = int((time.monotonic() - start) * 1000)
        return TaskResult(
            task_id=task["id"],
            variant=variant.name,
            run_index=run_index,
            success=success,
            wall_clock_ms=elapsed_ms,
            input_tokens=session_result.input_tokens if session_result else 0,
            output_tokens=session_result.output_tokens if session_result else 0,
            turns=session_turns,
            tool_calls=session_result.tool_calls if session_result else 0,
            error=error,
            metadata={
                "category": task["metadata"].get("category", BENCHMARK_NAME),
                "score": score,
                "expected_mechanism": score["expected_mechanism"],
            },
        )


def build_coordination_summary(results: list[TaskResult]) -> dict[str, Any]:
    """Build variant-level coordination metrics beyond generic pass/fail stats."""
    by_variant: dict[str, list[TaskResult]] = {}
    for result in results:
        by_variant.setdefault(result.variant, []).append(result)

    summary: dict[str, Any] = {}
    for variant, items in sorted(by_variant.items()):
        scores = [item.metadata.get("score", {}) for item in items]
        total = len(items)

        def avg_numeric(key: str) -> float:
            if total == 0:
                return 0.0
            return round(sum(float(score.get(key, 0.0)) for score in scores) / total, 3)

        def ratio_true(key: str) -> float:
            if total == 0:
                return 0.0
            return round(sum(1 for score in scores if score.get(key) is True) / total, 3)

        summary[variant] = {
            "runs": total,
            "coordination_passes": sum(1 for item in items if item.success),
            "coordination_pass_rate": ratio_true("coordination_success"),
            "avg_episode_inject_to_critic": avg_numeric("episode_inject_count_to_critic"),
            "episode_with_both_sources_rate": ratio_true("episode_inject_has_both_sources"),
            "avg_critic_doc_reads": avg_numeric("critic_doc_reads_total"),
            "avg_critic_required_doc_reads_after_write": avg_numeric("critic_doc_reads_after_required_writes"),
            "critic_finished_after_writes_rate": ratio_true("critic_ended_after_required_writes"),
            "output_has_both_markers_rate": ratio_true("content_has_both_markers"),
            "avg_citations_by_critic": avg_numeric("citations_by_critic"),
        }
    return summary


def write_coordination_reports(
    output_dir: Path,
    benchmark_name: str,
    config: BenchConfig,
    summary: dict[str, Any],
) -> None:
    """Write benchmark-specific coordination metric reports."""
    payload = {
        "benchmark": benchmark_name,
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "config": {
            "model": config.model,
            "runs_per_task": config.runs_per_task,
            "timeout": config.timeout,
        },
        "by_variant": summary,
    }
    (output_dir / "coordination.json").write_text(json.dumps(payload, indent=2) + "\n")

    lines: list[str] = []
    lines.append(f"# {benchmark_name} Coordination Metrics")
    lines.append("")
    lines.append("| Variant | Runs | Pass Rate | Episode(2-src) | Doc Reads (avg) | Doc Reads After Write (avg) | Critic End After Writes | Output Markers | Citations (avg) |")
    lines.append("|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    for variant, metrics in summary.items():
        lines.append(
            "| {variant} | {runs} | {pass_rate:.1%} | {episode_rate:.1%} | {doc_reads:.2f} | {doc_reads_after:.2f} | {end_after:.1%} | {markers:.1%} | {citations:.2f} |".format(
                variant=variant,
                runs=metrics["runs"],
                pass_rate=metrics["coordination_pass_rate"],
                episode_rate=metrics["episode_with_both_sources_rate"],
                doc_reads=metrics["avg_critic_doc_reads"],
                doc_reads_after=metrics["avg_critic_required_doc_reads_after_write"],
                end_after=metrics["critic_finished_after_writes_rate"],
                markers=metrics["output_has_both_markers_rate"],
                citations=metrics["avg_citations_by_critic"],
            )
        )
    lines.append("")
    (output_dir / "coordination.md").write_text("\n".join(lines))


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run coordination-routing benchmark")
    parser.add_argument("fixtures_dir", type=Path, help="Path to benchmark fixtures")
    parser.add_argument(
        "--variants",
        type=str,
        default=None,
        help="Comma-separated variants to run (default: all)",
    )
    parser.add_argument("--json", action="store_true", help="Print report JSON to stdout")
    BenchConfig.add_cli_args(parser)
    return parser


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()
    config = BenchConfig.from_cli(args)

    fixtures_dir: Path = args.fixtures_dir
    tasks = load_tasks(fixtures_dir)
    if not tasks:
        print(f"No tasks found in {fixtures_dir}", file=sys.stderr)
        sys.exit(1)

    variant_names = args.variants.split(",") if args.variants else None
    variants = get_variants(variant_names)

    total_runs = len(tasks) * len(variants) * config.runs_per_task
    print(f"Running {BENCHMARK_NAME}", file=sys.stderr)
    print(f"  Tasks: {[task['id'] for task in tasks]}", file=sys.stderr)
    print(f"  Variants: {[variant.name for variant in variants]}", file=sys.stderr)
    print(f"  Total runs: {total_runs}", file=sys.stderr)

    results: list[TaskResult] = []
    run_counter = 0
    for variant in variants:
        for task in tasks:
            for run_index in range(config.runs_per_task):
                run_counter += 1
                label = f"[{run_counter}/{total_runs}] {task['id']} / {variant.name} / run {run_index + 1}"
                print(label, file=sys.stderr)
                result = run_task(task, variant, run_index, config)
                results.append(result)
                status = "PASS" if result.success else "FAIL"
                score = result.metadata.get("score", {})
                print(
                    "  -> {status} | mechanism={mechanism} | markers={markers} | episodes={episodes} | reads={reads}".format(
                        status=status,
                        mechanism=score.get("expected_mechanism", "?"),
                        markers=score.get("content_has_both_markers", False),
                        episodes=score.get("episode_inject_count_to_critic", 0),
                        reads=score.get("critic_doc_reads_after_required_writes", 0),
                    ),
                    file=sys.stderr,
                )

    reporter = Reporter(BENCHMARK_NAME, results, config)

    if args.json:
        print(reporter.json())
        return

    output_dir = config.output_dir
    output_dir.mkdir(parents=True, exist_ok=True)
    reporter.write(output_dir)

    summary = build_coordination_summary(results)
    write_coordination_reports(output_dir, BENCHMARK_NAME, config, summary)

    print(f"Reports written to {output_dir}", file=sys.stderr)
    print(f"  - report.md / report.json", file=sys.stderr)
    print(f"  - coordination.md / coordination.json", file=sys.stderr)

    report_dict = json.loads(reporter.json())
    report_dict["coordination_summary"] = summary
    store = ResultStore(BENCHMARK_NAME)
    run_id = store.save(report_dict)
    print(f"Stored as run: {run_id}", file=sys.stderr)


if __name__ == "__main__":
    main()
