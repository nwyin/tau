"""JSON and markdown report generation from benchmark results.

The markdown output follows the reporting standard defined in
``benchmarks/TEMPLATE.md``.
"""

from __future__ import annotations

import json
from datetime import datetime, timezone
from pathlib import Path

from .config import BenchConfig
from .result import TaskResult


def _safe_div(a: int | float, b: int | float) -> float:
    """Divide *a* by *b*, returning 0.0 when *b* is zero."""
    return a / b if b else 0.0


def _aggregate(results: list[TaskResult]) -> dict:
    """Compute aggregate statistics for a list of results."""
    total = len(results)
    passed = sum(1 for r in results if r.success)
    total_input = sum(r.input_tokens for r in results)
    total_output = sum(r.output_tokens for r in results)
    total_tokens = total_input + total_output
    total_time = sum(r.wall_clock_ms for r in results)
    return {
        "total": total,
        "passed": passed,
        "pass_rate": round(_safe_div(passed, total), 4),
        "total_input_tokens": total_input,
        "total_output_tokens": total_output,
        "total_tokens": total_tokens,
        "total_time_ms": total_time,
        "avg_tokens": int(_safe_div(total_tokens, total)),
        "avg_time_ms": int(_safe_div(total_time, total)),
        "avg_turns": round(_safe_div(sum(r.turns for r in results), total), 2),
        "avg_tool_calls": round(_safe_div(sum(r.tool_calls for r in results), total), 2),
    }


class Reporter:
    """Generate JSON and markdown reports from benchmark results.

    Parameters:
        benchmark_name: Human-readable benchmark title used in report headers.
        results: List of :class:`TaskResult` instances to report on.
        config: The :class:`BenchConfig` used for this benchmark run.
    """

    def __init__(self, benchmark_name: str, results: list[TaskResult], config: BenchConfig) -> None:
        self.benchmark_name = benchmark_name
        self.results = results
        self.config = config
        self._timestamp = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    # ── slicing ──────────────────────────────────────────────────────

    def summary(self) -> dict:
        """Overall aggregate statistics across all results."""
        return _aggregate(self.results)

    def by_variant(self) -> dict[str, dict]:
        """Per-variant aggregate statistics."""
        groups: dict[str, list[TaskResult]] = {}
        for r in self.results:
            groups.setdefault(r.variant, []).append(r)
        return {name: _aggregate(rs) for name, rs in sorted(groups.items())}

    def by_category(self, key: str) -> dict[str, dict]:
        """Per-category aggregate statistics.

        Groups results by ``result.metadata[key]`` (e.g. ``"category"``,
        ``"difficulty"``, ``"language"``).
        """
        groups: dict[str, list[TaskResult]] = {}
        for r in self.results:
            category = r.metadata.get(key, "unknown")
            groups.setdefault(str(category), []).append(r)
        return {name: _aggregate(rs) for name, rs in sorted(groups.items())}

    # ── output formats ───────────────────────────────────────────────

    def json_dict(self, extra_fields: dict | None = None) -> dict:
        """Build the full report payload as a dictionary."""
        report: dict = {
            "benchmark": self.benchmark_name,
            "timestamp": self._timestamp,
            "config": {
                "model": self.config.model,
                "edit_mode": self.config.edit_mode,
                "runs_per_task": self.config.runs_per_task,
                "timeout": self.config.timeout,
                "concurrency": self.config.concurrency,
                "max_attempts": self.config.max_attempts,
            },
            "summary": self.summary(),
            "by_variant": self.by_variant(),
            "results": [r.to_dict() for r in self.results],
        }
        if extra_fields:
            report.update(extra_fields)
        return report

    def json(self) -> str:
        """Serialize the full report to a JSON string."""
        report = self.json_dict()
        return json.dumps(report, indent=2, ensure_ascii=False)

    def markdown(self) -> str:
        """Render a markdown report following the TEMPLATE.md format."""
        lines: list[str] = []

        # Header
        lines.append(f"# {self.benchmark_name} Results \u2014 {self._timestamp}")
        lines.append("")

        # Summary table
        s = self.summary()
        lines.append("## Summary")
        lines.append("| Metric | Value |")
        lines.append("|--------|-------|")
        lines.append(f"| Total runs | {s['total']} |")
        lines.append(f"| Passed | {s['passed']} |")
        lines.append(f"| Pass rate | {s['pass_rate']:.1%} |")
        lines.append(f"| Total input tokens | {s['total_input_tokens']:,} |")
        lines.append(f"| Total output tokens | {s['total_output_tokens']:,} |")
        lines.append(f"| Total time | {s['total_time_ms'] / 1000:.1f}s |")
        lines.append("")

        # By Variant table
        variants = self.by_variant()
        if variants:
            lines.append("## By Variant")
            lines.append("| Variant | Tasks | Passed | Rate | Avg Tokens | Avg Time |")
            lines.append("|---------|-------|--------|------|------------|----------|")
            for name, v in variants.items():
                lines.append(f"| {name} | {v['total']} | {v['passed']} | {v['pass_rate']:.1%} | {v['avg_tokens']:,} | {v['avg_time_ms']}ms |")
            lines.append("")

        # By Category table (only if results have category metadata)
        cats = self.by_category("category")
        has_categories = any(k != "unknown" for k in cats)
        if has_categories and len(variants) >= 2:
            variant_names = list(variants.keys())
            lines.append("## By Category")
            header = "| Category |"
            sep = "|----------|"
            for vn in variant_names:
                header += f" {vn} |"
                sep += "------|"
            lines.append(header)
            lines.append(sep)

            # Build per-category per-variant stats
            cat_variant: dict[str, dict[str, list[TaskResult]]] = {}
            for r in self.results:
                cat = r.metadata.get("category", "unknown")
                cat_variant.setdefault(cat, {}).setdefault(r.variant, []).append(r)

            for cat in sorted(cat_variant):
                if cat == "unknown":
                    continue
                row = f"| {cat} |"
                for vn in variant_names:
                    rs = cat_variant[cat].get(vn, [])
                    if rs:
                        p = sum(1 for r in rs if r.success)
                        row += f" {p}/{len(rs)} ({_safe_div(p, len(rs)):.0%}) |"
                    else:
                        row += " - |"
                lines.append(row)
            lines.append("")
        elif has_categories:
            lines.append("## By Category")
            lines.append("| Category | Tasks | Passed | Rate |")
            lines.append("|----------|-------|--------|------|")
            for cat, cs in cats.items():
                if cat == "unknown":
                    continue
                lines.append(f"| {cat} | {cs['total']} | {cs['passed']} | {cs['pass_rate']:.1%} |")
            lines.append("")

        # Failures (first 10)
        failures = [r for r in self.results if not r.success]
        if failures:
            lines.append("## Failures (first 10)")
            lines.append("| Task | Variant | Error |")
            lines.append("|------|---------|-------|")
            for r in failures[:10]:
                error = r.error or ""
                # Truncate long errors for the table
                if len(error) > 80:
                    error = error[:77] + "..."
                # Escape pipes in error text
                error = error.replace("|", "\\|")
                lines.append(f"| {r.task_id} | {r.variant} | {error} |")
            lines.append("")

        return "\n".join(lines)

    def write(self, output_dir: Path, extra_fields: dict | None = None) -> None:
        """Write ``report.md`` and ``report.json`` to *output_dir*."""
        output_dir.mkdir(parents=True, exist_ok=True)
        (output_dir / "report.md").write_text(self.markdown())
        payload = self.json_dict(extra_fields=extra_fields)
        (output_dir / "report.json").write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n")
