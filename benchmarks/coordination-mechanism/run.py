#!/usr/bin/env python3
"""Runner for coordination-mechanism benchmark."""

from __future__ import annotations

import argparse
import json
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.config import BenchConfig
from shared.coordination import CoordinationExpectations, load_coordination_tasks
from shared.coordination_runner import (
    CoordinationReportColumn,
    avg_score,
    ratio_score,
    run_coordination_benchmark,
)
from shared.result import TaskResult
from shared.session import SessionResult, TauSession
from shared.variants import Variant

from mechanism_score import score_from_trace_file
from variants import get_variants

BENCHMARK_NAME = "coordination-mechanism"

EXTRA_METRICS = {
    "scaffold_fidelity_pass_rate": ratio_score("scaffold_fidelity_success"),
    "final_doc_written_rate": ratio_score("final_doc_written"),
    "avg_top_level_tool_count": avg_score("top_level_tool_count"),
}

REPORT_COLUMNS = [
    CoordinationReportColumn("Runs", "runs", "int"),
    CoordinationReportColumn("Official", "official_pass_rate"),
    CoordinationReportColumn("Session", "session_pass_rate"),
    CoordinationReportColumn("Coordination", "coordination_pass_rate"),
    CoordinationReportColumn("Scaffold", "scaffold_fidelity_pass_rate"),
    CoordinationReportColumn("Mechanism", "mechanism_pass_rate"),
    CoordinationReportColumn("Timing", "timing_pass_rate"),
    CoordinationReportColumn("Synthesis", "synthesis_pass_rate"),
    CoordinationReportColumn("Final Doc", "final_doc_written_rate"),
    CoordinationReportColumn("Top-Level Tools (avg)", "avg_top_level_tool_count", "float2"),
    CoordinationReportColumn("Episode(2-src)", "episode_with_both_sources_rate"),
    CoordinationReportColumn("Doc Reads After Write (avg)", "avg_critic_required_doc_reads_after_write", "float2"),
]

EXECUTION_SYSTEM_PROMPT = """You are executing a benchmark-owned orchestration scaffold.

Requirements:
- Your first and only top-level tool call must be `py_repl`.
- Execute the provided Python scaffold exactly as written.
- Do not call `thread`, `query`, `document`, or any other tool directly from the main agent.
- After the scaffold finishes, reply with exactly `DONE`.
"""


def build_critic_task(expectations: CoordinationExpectations, variant_name: str) -> str:
    """Build the critic task text for a specific topology variant."""
    base = expectations.critic_task.strip()
    if variant_name == "prompt-only-parallel":
        extra = f"""
You are launched in the same parallel batch as both upstream workers.
Before completing, wait until both `{expectations.pro_doc}` and `{expectations.con_doc}` exist.
Read both documents after they are written, then critique them.
Do not finish before both reads have happened.
"""
    elif variant_name == "staged-pipeline":
        extra = """
You will receive both upstream workers as episodes.
Base the critique on those upstream artifacts before you synthesize.
"""
    elif variant_name == "document-polling":
        pro_examples = ", ".join(f"`{marker}`" for marker in expectations.pro_markers[:2])
        con_examples = ", ".join(f"`{marker}`" for marker in expectations.con_markers[:2])
        extra = f"""
You are launched in the same parallel batch as both upstream workers.
Poll `{expectations.pro_doc}` and `{expectations.con_doc}` until both contain anchor facts.
Specifically, do not finish until you have read `{expectations.pro_doc}` after it contains one of {pro_examples}
and `{expectations.con_doc}` after it contains one of {con_examples}.
Do not finish before both reads have happened.
"""
    else:
        extra = ""

    return f"{base}\n{extra}".strip()


def build_variant_scaffold(expectations: CoordinationExpectations, variant_name: str) -> str:
    """Generate the exact py_repl scaffold for a task/variant pair."""

    if not expectations.pro_task or not expectations.con_task or not expectations.critic_task:
        raise ValueError("coordination-mechanism fixtures require explicit pro_task/con_task/critic_task")

    critic_task = build_critic_task(expectations, variant_name)

    def py_str(value: str) -> str:
        return json.dumps(value, ensure_ascii=False)

    lines = [
        f"pro_alias = {py_str(expectations.pro_alias)}",
        f"con_alias = {py_str(expectations.con_alias)}",
        f"critic_alias = {py_str(expectations.critic_alias)}",
        f"pro_doc = {py_str(expectations.pro_doc)}",
        f"con_doc = {py_str(expectations.con_doc)}",
        f"final_doc = {py_str(expectations.final_doc)}",
        f"pro_task = {py_str(expectations.pro_task)}",
        f"con_task = {py_str(expectations.con_task)}",
        f"critic_task = {py_str(critic_task)}",
        "",
    ]

    if variant_name in {"naive-parallel", "prompt-only-parallel", "document-polling"}:
        critic_turns = 16 if variant_name == "document-polling" else 12
        lines.extend(
            [
                "results = tau.parallel(",
                "    tau.Thread(pro_alias, pro_task, max_turns=8),",
                "    tau.Thread(con_alias, con_task, max_turns=8),",
                f"    tau.Thread(critic_alias, critic_task, max_turns={critic_turns}),",
                ")",
                "critic_result = results[2]",
            ]
        )
    elif variant_name == "staged-pipeline":
        lines.extend(
            [
                "tau.parallel(",
                "    tau.Thread(pro_alias, pro_task, max_turns=8),",
                "    tau.Thread(con_alias, con_task, max_turns=8),",
                ")",
                "critic_result = tau.thread(",
                "    critic_alias,",
                "    critic_task,",
                "    episodes=[pro_alias, con_alias],",
                "    max_turns=12,",
                ")",
            ]
        )
    else:
        raise ValueError(f"unknown coordination variant: {variant_name}")

    if expectations.synthesis_task:
        lines.extend(
            [
                f"synthesis_task = {py_str(expectations.synthesis_task)}",
                "final_text = tau.query(",
                "    synthesis_task",
                "    + \"\\n\\nPrimary artifact:\\n\" + tau.document('read', name=pro_doc)",
                "    + \"\\n\\nSecondary artifact:\\n\" + tau.document('read', name=con_doc)",
                '    + "\\n\\nCritic output:\\n" + critic_result.output',
                ")",
            ]
        )
    else:
        lines.append("final_text = critic_result.output")

    lines.extend(
        [
            "tau.document(operation='write', name=final_doc, content=final_text)",
            "print('DONE')",
        ]
    )
    return "\n".join(lines)


def build_execution_prompt(task: dict[str, Any], variant_name: str, scaffold: str) -> str:
    """Build the main-agent prompt for a scaffold-owned benchmark run."""
    return f"""Benchmark: `{BENCHMARK_NAME}`
Fixture: `{task["id"]}`
Variant: `{variant_name}`

Execute the following Python scaffold in a single `py_repl` tool call.
Copy the scaffold exactly as written. Do not add setup code, comments, or extra tool calls.

```python
{scaffold}
```

After the tool finishes, reply with exactly `DONE`.
""".strip()


def run_task(task: dict[str, Any], variant: Variant, run_index: int, config: BenchConfig) -> TaskResult:
    """Run a single task/variant/run combination."""
    start = time.monotonic()
    scaffold = build_variant_scaffold(task["expectations"], variant.name)
    prompt = build_execution_prompt(task, variant.name, scaffold)
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
                session_result = session.send(prompt, system=EXECUTION_SYSTEM_PROMPT)
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
            expected_scaffold=scaffold,
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
        "  -> {status} | session_ok={session_ok} | scaffold_ok={scaffold_ok} | mechanism_ok={mechanism_ok} | "
        "timing_ok={timing_ok} | synthesis_ok={synthesis_ok} | final_doc={final_doc} | top_level_tools={top_tools}"
    ).format(
        status=status,
        session_ok=result.metadata.get("session_success", False),
        scaffold_ok=score.get("scaffold_fidelity_success", False),
        mechanism_ok=score.get("mechanism_success", False),
        timing_ok=score.get("timing_success", False),
        synthesis_ok=score.get("synthesis_success", False),
        final_doc=score.get("final_doc_written", False),
        top_tools=score.get("top_level_tool_count", 0),
    )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run coordination-mechanism benchmark")
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
    )


if __name__ == "__main__":
    main()
