"""Coordination overhead analysis for subagent-decomposition benchmark.

Reads report.json and computes per-variant metrics, compares single-agent
vs sub-agent strategies, and identifies the crossover point across
difficulty levels.

Usage:
    python analyze.py results/report.json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def load_report(path: Path) -> dict:
    """Load a benchmark report JSON file."""
    return json.loads(path.read_text())


def compute_variant_stats(results: list[dict]) -> dict[str, dict]:
    """Compute per-variant statistics from result entries."""
    by_variant: dict[str, list[dict]] = {}
    for r in results:
        variant = r.get("variant", "unknown")
        by_variant.setdefault(variant, []).append(r)

    stats: dict[str, dict] = {}
    for variant, entries in sorted(by_variant.items()):
        total = len(entries)
        passed = sum(1 for r in entries if r.get("success", False))
        total_tokens = sum(r.get("input_tokens", 0) + r.get("output_tokens", 0) for r in entries)
        total_time = sum(r.get("wall_clock_ms", 0) for r in entries)

        # Metadata-based metrics
        rework_rates = [r.get("metadata", {}).get("rework_rate", 0.0) for r in entries]
        coord_failures = sum(1 for r in entries if r.get("metadata", {}).get("coordination_failure", False))
        callers_correct = sum(r.get("metadata", {}).get("callers_correct", 0) for r in entries)
        callers_total = sum(r.get("metadata", {}).get("callers_total", 0) for r in entries)

        stats[variant] = {
            "total": total,
            "passed": passed,
            "success_rate": passed / max(total, 1),
            "avg_tokens": total_tokens / max(total, 1),
            "avg_time_ms": total_time / max(total, 1),
            "rework_rate": sum(rework_rates) / max(len(rework_rates), 1),
            "coordination_failure_rate": coord_failures / max(total, 1),
            "caller_accuracy": callers_correct / max(callers_total, 1),
        }

    return stats


def compute_difficulty_breakdown(results: list[dict]) -> dict[str, dict[str, dict]]:
    """Compute per-difficulty per-variant statistics."""
    by_difficulty: dict[str, list[dict]] = {}
    for r in results:
        difficulty = r.get("task_id", "unknown")  # task_id = difficulty level name
        by_difficulty.setdefault(difficulty, []).append(r)

    breakdown: dict[str, dict[str, dict]] = {}
    for difficulty, entries in sorted(by_difficulty.items()):
        breakdown[difficulty] = compute_variant_stats(entries)

    return breakdown


def find_crossover(breakdown: dict[str, dict[str, dict]]) -> list[str]:
    """Identify difficulty levels where sub-agent variants beat single-agent."""
    insights: list[str] = []
    difficulty_order = ["easy", "medium", "hard"]

    for difficulty in difficulty_order:
        if difficulty not in breakdown:
            continue
        variant_stats = breakdown[difficulty]
        single = variant_stats.get("single-agent", {})
        single_rate = single.get("success_rate", 0)

        for variant_name in ["sub-msg", "sub-discover", "hive"]:
            if variant_name not in variant_stats:
                continue
            sub_rate = variant_stats[variant_name].get("success_rate", 0)
            sub_tokens = variant_stats[variant_name].get("avg_tokens", 0)
            single_tokens = single.get("avg_tokens", 0)

            if sub_rate > single_rate:
                token_overhead = ((sub_tokens - single_tokens) / max(single_tokens, 1)) * 100
                insights.append(
                    f"  {difficulty}: {variant_name} beats single-agent "
                    f"({sub_rate:.0%} vs {single_rate:.0%}, "
                    f"+{token_overhead:.0f}% token overhead)"
                )
            elif sub_rate == single_rate and sub_rate > 0:
                insights.append(f"  {difficulty}: {variant_name} matches single-agent ({sub_rate:.0%})")

    if not insights:
        insights.append("  No crossover found: single-agent dominates across all difficulty levels")

    return insights


def format_report(stats: dict[str, dict], breakdown: dict[str, dict[str, dict]], crossover: list[str]) -> str:
    """Format analysis as readable text."""
    lines: list[str] = []

    lines.append("=" * 80)
    lines.append("Subagent Decomposition Analysis")
    lines.append("=" * 80)
    lines.append("")

    # Overall variant comparison
    lines.append("## Overall Variant Comparison")
    lines.append("")
    header = f"{'Variant':<16} {'Success%':>9} {'Avg Tokens':>12} {'Avg Time':>10} {'Re-work%':>9} {'Coord Fail%':>12} {'Caller Acc':>11}"
    lines.append(header)
    lines.append("-" * len(header))

    for variant, s in stats.items():
        lines.append(
            f"{variant:<16} {s['success_rate']:>8.0%} {s['avg_tokens']:>12,.0f} "
            f"{s['avg_time_ms'] / 1000:>9.1f}s {s['rework_rate']:>8.0%} "
            f"{s['coordination_failure_rate']:>11.0%} {s['caller_accuracy']:>10.0%}"
        )

    lines.append("")

    # Per-difficulty breakdown
    lines.append("## By Difficulty Level")
    lines.append("")
    for difficulty in ["easy", "medium", "hard"]:
        if difficulty not in breakdown:
            continue
        lines.append(f"### {difficulty.title()}")
        d_stats = breakdown[difficulty]
        for variant, s in d_stats.items():
            lines.append(
                f"  {variant:<16} success={s['success_rate']:.0%}  "
                f"tokens={s['avg_tokens']:,.0f}  "
                f"time={s['avg_time_ms'] / 1000:.1f}s  "
                f"callers={s['caller_accuracy']:.0%}"
            )
        lines.append("")

    # Crossover analysis
    lines.append("## Crossover Analysis")
    lines.append("(Where do sub-agents start to help?)")
    lines.append("")
    for insight in crossover:
        lines.append(insight)
    lines.append("")

    # Key takeaways
    lines.append("## Key Takeaways")
    lines.append("")

    # Compare single-agent vs best sub-agent
    if "single-agent" in stats:
        single_rate = stats["single-agent"]["success_rate"]
        best_sub = None
        best_sub_rate = 0
        for v, s in stats.items():
            if v != "single-agent" and v != "hive" and s["success_rate"] > best_sub_rate:
                best_sub = v
                best_sub_rate = s["success_rate"]

        if best_sub:
            delta = best_sub_rate - single_rate
            if delta > 0:
                lines.append(f"- Best sub-agent ({best_sub}) beats single-agent by {delta:.0%}")
            elif delta == 0:
                lines.append(f"- Best sub-agent ({best_sub}) matches single-agent at {single_rate:.0%}")
            else:
                lines.append(f"- Single-agent beats all sub-agent variants (by {-delta:.0%})")
                lines.append("  -> Coordination overhead exceeds parallelism benefit")

        # Token efficiency
        single_tokens = stats["single-agent"]["avg_tokens"]
        for v in ["sub-msg", "sub-discover"]:
            if v in stats:
                sub_tokens = stats[v]["avg_tokens"]
                overhead = ((sub_tokens - single_tokens) / max(single_tokens, 1)) * 100
                lines.append(f"- {v} token overhead vs single-agent: {overhead:+.0f}%")

    lines.append("")
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description="Analyze subagent-decomposition benchmark results")
    parser.add_argument("report", type=Path, help="Path to report.json")
    parser.add_argument("--json", action="store_true", help="Output raw JSON stats")
    args = parser.parse_args()

    if not args.report.exists():
        print(f"Report not found: {args.report}")
        sys.exit(1)

    report = load_report(args.report)
    results = report.get("results", [])

    if not results:
        print("No results found in report")
        sys.exit(1)

    stats = compute_variant_stats(results)
    breakdown = compute_difficulty_breakdown(results)
    crossover = find_crossover(breakdown)

    if args.json:
        output = {
            "variant_stats": stats,
            "difficulty_breakdown": breakdown,
            "crossover_insights": crossover,
        }
        print(json.dumps(output, indent=2))
    else:
        print(format_report(stats, breakdown, crossover))


if __name__ == "__main__":
    main()
