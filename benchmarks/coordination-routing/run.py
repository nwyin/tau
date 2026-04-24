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
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.config import BenchConfig
from shared.coordination import load_coordination_tasks
from shared.coordination_runner import (
    CoordinationReportColumn,
    avg_score,
    ratio_score,
    run_coordination_benchmark,
)
from shared.result import TaskResult
from shared.session import SessionResult, TauSession
from shared.variants import Variant

from score import CoordinationExpectations, score_from_trace_file
from variants import get_variants

BENCHMARK_NAME = "coordination-routing"

EXTRA_METRICS = {
    "requested_shape_follow_rate": ratio_score("requested_shape_followed"),
    "variant_escape_rate": ratio_score("variant_escape"),
    "self_corrected_rate": ratio_score("self_corrected_to_other_shape"),
    "avg_critic_doc_reads": avg_score("critic_doc_reads_total"),
    "critic_finished_after_writes_rate": ratio_score("critic_ended_after_required_writes"),
    "output_has_both_markers_rate": ratio_score("content_has_both_markers"),
    "avg_citations_by_critic": avg_score("citations_by_critic"),
}

REPORT_COLUMNS = [
    CoordinationReportColumn("Runs", "runs", "int"),
    CoordinationReportColumn("Official", "official_pass_rate"),
    CoordinationReportColumn("Session", "session_pass_rate"),
    CoordinationReportColumn("Coordination", "coordination_pass_rate"),
    CoordinationReportColumn("Shape Followed", "requested_shape_follow_rate"),
    CoordinationReportColumn("Escape", "variant_escape_rate"),
    CoordinationReportColumn("Self-Corrected", "self_corrected_rate"),
    CoordinationReportColumn("Mechanism", "mechanism_pass_rate"),
    CoordinationReportColumn("Timing", "timing_pass_rate"),
    CoordinationReportColumn("Synthesis", "synthesis_pass_rate"),
    CoordinationReportColumn("Episode(2-src)", "episode_with_both_sources_rate"),
    CoordinationReportColumn("Doc Reads After Write (avg)", "avg_critic_required_doc_reads_after_write", "float2"),
]


def build_variant_prompt(task_prompt: str, variant_name: str, exp: CoordinationExpectations) -> str:
    """Build the benchmark prompt for a specific orchestration variant."""
    pro_examples = ", ".join(f"`{marker}`" for marker in exp.pro_markers[:2])
    con_examples = ", ".join(f"`{marker}`" for marker in exp.con_markers[:2])
    base = f"""{task_prompt}

Execution requirements (all variants):
- Use these exact thread aliases: `{exp.pro_alias}`, `{exp.con_alias}`, `{exp.critic_alias}`.
- `{exp.pro_alias}` must write to document `{exp.pro_doc}`.
- `{exp.con_alias}` must write to document `{exp.con_doc}`.
- Final response must include at least one anchor from `{exp.pro_alias}` ({pro_examples}) and at least one anchor from `{exp.con_alias}` ({con_examples}).
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
    session_timeout = variant.timeout(config.timeout)

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
                edit_mode=config.edit_mode,
                trace_output=trace_dir,
                task_id=task_id,
                timeout=session_timeout,
                tau_binary=config.tau_binary,
            ) as session:
                session_result = session.send(prompt)
                session_turns = session.turns
        except Exception as exc:
            error = str(exc)

        output_text = session_result.output if session_result else ""
        session_success = bool(session_result) and not output_text.startswith("error:") and error is None
        score = score_from_trace_file(
            trace_path=trace_file,
            output_text=output_text,
            variant_name=variant.name,
            expectations=task["expectations"],
        )

        coordination_success = bool(score["coordination_success"])
        success = coordination_success and session_success
        if error is None and not coordination_success:
            error = score["success_reason"]
        if session_result and session_result.output.startswith("error:"):
            error = session_result.output

        elapsed_ms = int((time.monotonic() - start) * 1000)
        return TaskResult.from_session(
            task_id=task["id"],
            variant=variant.name,
            run_index=run_index,
            success=success,
            wall_clock_ms=elapsed_ms,
            session_result=session_result,
            turns=session_turns,
            error=error,
            metadata={
                "category": BENCHMARK_NAME,
                "score": score,
                "expected_mechanism": score["expected_mechanism"],
                "session_success": session_success,
            },
        )


def status_line(result: TaskResult) -> str:
    """Format per-run progress for stderr."""
    status = "PASS" if result.success else "FAIL"
    score = result.metadata.get("score", {})
    return (
        "  -> {status} | session_ok={session_ok} | mechanism_ok={mechanism_ok} | timing_ok={timing_ok} | "
        "shape_followed={shape_ok} | synthesis_ok={synthesis_ok} | expected={mechanism} | episodes={episodes} | reads={reads}"
    ).format(
        status=status,
        session_ok=result.metadata.get("session_success", False),
        mechanism_ok=score.get("mechanism_success", False),
        timing_ok=score.get("timing_success", False),
        shape_ok=score.get("requested_shape_followed", False),
        synthesis_ok=score.get("synthesis_success", False),
        mechanism=score.get("expected_mechanism", "?"),
        episodes=score.get("episode_inject_count_to_critic", 0),
        reads=score.get("critic_doc_reads_after_required_writes", 0),
    )


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
    tasks = load_coordination_tasks(fixtures_dir)
    if not tasks:
        print(f"No tasks found in {fixtures_dir}", file=sys.stderr)
        sys.exit(1)

    variant_names = args.variants.split(",") if args.variants else None
    variants = get_variants(variant_names)

    run_coordination_benchmark(
        benchmark_name=BENCHMARK_NAME,
        tasks=tasks,
        variants=variants,
        config=config,
        json_output=args.json,
        run_task=run_task,
        status_line=status_line,
        extra_metrics=EXTRA_METRICS,
        report_columns=REPORT_COLUMNS,
        include_official_passes=True,
    )


if __name__ == "__main__":
    main()
