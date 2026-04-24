#!/usr/bin/env python3
"""Runner for coordination-mechanism benchmark."""

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
from shared.coordination import CoordinationExpectations, load_coordination_tasks
from shared.reporter import Reporter
from shared.result import TaskResult
from shared.session import SessionResult, TauSession
from shared.store import ResultStore
from shared.variants import Variant

from mechanism_score import score_from_trace_file
from variants import get_variants

BENCHMARK_NAME = "coordination-mechanism"

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
                "    + \"\\n\\nCritic output:\\n\" + critic_result.output",
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
Fixture: `{task['id']}`
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
    session_timeout = int(variant.tau_config_overrides.get("timeout", config.timeout))

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
                "category": BENCHMARK_NAME,
                "score": score,
                "expected_mechanism": score["expected_mechanism"],
                "session_success": session_success,
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
            "official_pass_rate": round(sum(1 for item in items if item.success) / total, 3) if total else 0.0,
            "session_pass_rate": round(
                sum(1 for item in items if item.metadata.get("session_success") is True) / total,
                3,
            )
            if total
            else 0.0,
            "coordination_pass_rate": ratio_true("coordination_success"),
            "scaffold_fidelity_pass_rate": ratio_true("scaffold_fidelity_success"),
            "mechanism_pass_rate": ratio_true("mechanism_success"),
            "timing_pass_rate": ratio_true("timing_success"),
            "synthesis_pass_rate": ratio_true("synthesis_success"),
            "final_doc_written_rate": ratio_true("final_doc_written"),
            "avg_top_level_tool_count": avg_numeric("top_level_tool_count"),
            "avg_episode_inject_to_critic": avg_numeric("episode_inject_count_to_critic"),
            "episode_with_both_sources_rate": ratio_true("episode_inject_has_both_sources"),
            "avg_critic_required_doc_reads_after_write": avg_numeric("critic_doc_reads_after_required_writes"),
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
    lines.append("| Variant | Runs | Official | Session | Coordination | Scaffold | Mechanism | Timing | Synthesis | Final Doc | Top-Level Tools (avg) | Episode(2-src) | Doc Reads After Write (avg) |")
    lines.append("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    for variant, metrics in summary.items():
        lines.append(
            "| {variant} | {runs} | {official:.1%} | {session:.1%} | {coordination:.1%} | {scaffold:.1%} | {mechanism:.1%} | {timing:.1%} | {synthesis:.1%} | {final_doc:.1%} | {top_tools:.2f} | {episode_rate:.1%} | {doc_reads_after:.2f} |".format(
                variant=variant,
                runs=metrics["runs"],
                official=metrics["official_pass_rate"],
                session=metrics["session_pass_rate"],
                coordination=metrics["coordination_pass_rate"],
                scaffold=metrics["scaffold_fidelity_pass_rate"],
                mechanism=metrics["mechanism_pass_rate"],
                timing=metrics["timing_pass_rate"],
                synthesis=metrics["synthesis_pass_rate"],
                final_doc=metrics["final_doc_written_rate"],
                top_tools=metrics["avg_top_level_tool_count"],
                episode_rate=metrics["episode_with_both_sources_rate"],
                doc_reads_after=metrics["avg_critic_required_doc_reads_after_write"],
            )
        )
    lines.append("")
    (output_dir / "coordination.md").write_text("\n".join(lines))


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
                    "  -> {status} | session_ok={session_ok} | scaffold_ok={scaffold_ok} | mechanism_ok={mechanism_ok} | timing_ok={timing_ok} | synthesis_ok={synthesis_ok} | final_doc={final_doc} | top_level_tools={top_tools}".format(
                        status=status,
                        session_ok=result.metadata.get("session_success", False),
                        scaffold_ok=score.get("scaffold_fidelity_success", False),
                        mechanism_ok=score.get("mechanism_success", False),
                        timing_ok=score.get("timing_success", False),
                        synthesis_ok=score.get("synthesis_success", False),
                        final_doc=score.get("final_doc_written", False),
                        top_tools=score.get("top_level_tool_count", 0),
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
    print("  - report.md / report.json", file=sys.stderr)
    print("  - coordination.md / coordination.json", file=sys.stderr)

    report_dict = json.loads(reporter.json())
    report_dict["coordination_summary"] = summary
    store = ResultStore(BENCHMARK_NAME)
    run_id = store.save(report_dict)
    print(f"Stored as run: {run_id}", file=sys.stderr)


if __name__ == "__main__":
    main()
