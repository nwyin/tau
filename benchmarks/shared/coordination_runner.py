"""Shared runner/report helpers for coordination benchmarks."""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable

from .config import BenchConfig
from .reporter import Reporter
from .result import TaskResult
from .store import ResultStore
from .variants import Variant

RunTaskFn = Callable[[dict[str, Any], Variant, int, BenchConfig], TaskResult]
StatusLineFn = Callable[[TaskResult], str]
ExtraMetricFn = Callable[[list[TaskResult], list[dict[str, Any]], int], Any]


@dataclass(frozen=True)
class CoordinationReportColumn:
    """A metric column in coordination.md."""

    label: str
    key: str
    fmt: str = "percent"

    def render(self, metrics: dict[str, Any]) -> str:
        value = metrics[self.key]
        if self.fmt == "percent":
            return f"{float(value):.1%}"
        if self.fmt == "float2":
            return f"{float(value):.2f}"
        if self.fmt == "int":
            return str(int(value))
        return str(value)


def avg_score(key: str) -> ExtraMetricFn:
    """Return a metric function averaging a numeric score field."""

    def metric(_items: list[TaskResult], scores: list[dict[str, Any]], total: int) -> float:
        if total == 0:
            return 0.0
        return round(sum(float(score.get(key, 0.0)) for score in scores) / total, 3)

    return metric


def ratio_score(key: str) -> ExtraMetricFn:
    """Return a metric function computing the ratio of true score fields."""

    def metric(_items: list[TaskResult], scores: list[dict[str, Any]], total: int) -> float:
        if total == 0:
            return 0.0
        return round(sum(1 for score in scores if score.get(key) is True) / total, 3)

    return metric


def build_coordination_summary(
    results: list[TaskResult],
    *,
    extra_metrics: dict[str, ExtraMetricFn] | None = None,
    include_official_passes: bool = False,
) -> dict[str, Any]:
    """Build variant-level coordination metrics from TaskResult metadata."""
    by_variant: dict[str, list[TaskResult]] = {}
    for result in results:
        by_variant.setdefault(result.variant, []).append(result)

    summary: dict[str, Any] = {}
    for variant, items in sorted(by_variant.items()):
        scores = [item.metadata.get("score", {}) for item in items]
        total = len(items)

        official_passes = sum(1 for item in items if item.success)
        metrics: dict[str, Any] = {
            "runs": total,
            "official_pass_rate": round(official_passes / total, 3) if total else 0.0,
            "session_pass_rate": round(
                sum(1 for item in items if item.metadata.get("session_success") is True) / total,
                3,
            )
            if total
            else 0.0,
            "coordination_pass_rate": ratio_score("coordination_success")(items, scores, total),
            "mechanism_pass_rate": ratio_score("mechanism_success")(items, scores, total),
            "timing_pass_rate": ratio_score("timing_success")(items, scores, total),
            "synthesis_pass_rate": ratio_score("synthesis_success")(items, scores, total),
            "avg_episode_inject_to_critic": avg_score("episode_inject_count_to_critic")(items, scores, total),
            "episode_with_both_sources_rate": ratio_score("episode_inject_has_both_sources")(items, scores, total),
            "avg_critic_required_doc_reads_after_write": avg_score("critic_doc_reads_after_required_writes")(items, scores, total),
        }
        if include_official_passes:
            metrics["official_passes"] = official_passes
        for key, metric in (extra_metrics or {}).items():
            metrics[key] = metric(items, scores, total)
        summary[variant] = metrics
    return summary


def write_coordination_reports(
    output_dir: Path,
    benchmark_name: str,
    config: BenchConfig,
    summary: dict[str, Any],
    columns: list[CoordinationReportColumn],
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

    lines: list[str] = [
        f"# {benchmark_name} Coordination Metrics",
        "",
        "| Variant | " + " | ".join(column.label for column in columns) + " |",
        "|---|" + "|".join("---:" for _ in columns) + "|",
    ]
    for variant, metrics in summary.items():
        rendered = " | ".join(column.render(metrics) for column in columns)
        lines.append(f"| {variant} | {rendered} |")
    lines.append("")
    (output_dir / "coordination.md").write_text("\n".join(lines))


def run_coordination_benchmark(
    *,
    benchmark_name: str,
    tasks: list[dict[str, Any]],
    variants: list[Variant],
    config: BenchConfig,
    json_output: bool,
    run_task: RunTaskFn,
    status_line: StatusLineFn,
    extra_metrics: dict[str, ExtraMetricFn] | None,
    report_columns: list[CoordinationReportColumn],
    include_official_passes: bool = False,
) -> list[TaskResult]:
    """Run the standard coordination benchmark loop and persist reports."""
    total_runs = len(tasks) * len(variants) * config.runs_per_task
    print(f"Running {benchmark_name}", file=sys.stderr)
    print(f"  Tasks: {[task['id'] for task in tasks]}", file=sys.stderr)
    print(f"  Variants: {[variant.name for variant in variants]}", file=sys.stderr)
    print(f"  Total runs: {total_runs}", file=sys.stderr)

    results: list[TaskResult] = []
    run_counter = 0
    for variant in variants:
        for task in tasks:
            for run_index in range(config.runs_per_task):
                run_counter += 1
                print(f"[{run_counter}/{total_runs}] {task['id']} / {variant.name} / run {run_index + 1}", file=sys.stderr)
                result = run_task(task, variant, run_index, config)
                results.append(result)
                print(status_line(result), file=sys.stderr)

    reporter = Reporter(benchmark_name, results, config)
    if json_output:
        print(reporter.json())
        return results

    output_dir = config.output_dir
    output_dir.mkdir(parents=True, exist_ok=True)
    summary = build_coordination_summary(
        results,
        extra_metrics=extra_metrics,
        include_official_passes=include_official_passes,
    )
    reporter.write(output_dir)
    write_coordination_reports(output_dir, benchmark_name, config, summary, report_columns)

    print(f"Reports written to {output_dir}", file=sys.stderr)
    print("  - report.md / report.json", file=sys.stderr)
    print("  - coordination.md / coordination.json", file=sys.stderr)

    report_dict = reporter.json_dict(extra_fields={"coordination_summary": summary})
    run_id = ResultStore(benchmark_name).save(report_dict)
    print(f"Stored as run: {run_id}", file=sys.stderr)
    return results
