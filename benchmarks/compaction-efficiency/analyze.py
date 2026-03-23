#!/usr/bin/env python3
"""Pareto frontier analysis for compaction-efficiency benchmark.

Reads a report.json and computes the Pareto frontier: configurations not
dominated on (compression_ratio, success_rate).  A configuration is dominated
if another config has both better success rate AND better compression.

Usage:
    python analyze.py results/report.json
    python analyze.py results/report.json -o results/pareto.json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Data structures
# ---------------------------------------------------------------------------


def _aggregate_by_variant(report: dict) -> list[dict]:
    """Aggregate results by variant, computing success rate and token stats."""
    by_variant: dict[str, list[dict]] = {}
    for result in report.get("results", []):
        variant = result.get("variant", "unknown")
        by_variant.setdefault(variant, []).append(result)

    aggregated: list[dict] = []
    for variant, results in sorted(by_variant.items()):
        total = len(results)
        successes = sum(1 for r in results if r.get("success", False))
        success_rate = successes / total if total > 0 else 0.0

        avg_input = sum(r.get("input_tokens", 0) for r in results) / total if total else 0
        avg_output = sum(r.get("output_tokens", 0) for r in results) / total if total else 0
        avg_tokens = avg_input + avg_output
        avg_time = sum(r.get("wall_clock_ms", 0) for r in results) / total if total else 0

        # Extract compression ratio from metadata if available
        compression_ratios = [r.get("metadata", {}).get("compression_ratio", 1.0) for r in results if "metadata" in r]
        avg_compression = sum(compression_ratios) / len(compression_ratios) if compression_ratios else 1.0

        # Extract compaction overhead from metadata
        overheads = [r.get("metadata", {}).get("compaction_overhead_ms", 0) for r in results if "metadata" in r]
        avg_overhead = sum(overheads) / len(overheads) if overheads else 0

        aggregated.append(
            {
                "variant": variant,
                "total_runs": total,
                "success_rate": round(success_rate, 4),
                "avg_tokens": round(avg_tokens),
                "avg_input_tokens": round(avg_input),
                "avg_output_tokens": round(avg_output),
                "avg_time_ms": round(avg_time),
                "compression_ratio": round(avg_compression, 4),
                "compaction_overhead_ms": round(avg_overhead),
            }
        )

    return aggregated


# ---------------------------------------------------------------------------
# Pareto frontier
# ---------------------------------------------------------------------------


def compute_pareto_frontier(aggregated: list[dict]) -> list[dict]:
    """Compute the Pareto frontier on (compression_ratio, success_rate).

    A point is on the frontier if no other point has both:
      - lower compression_ratio (more compression is better)
      - higher success_rate

    Points on the frontier represent the best tradeoff between compression
    and task success.
    """
    frontier: list[dict] = []

    for point in aggregated:
        dominated = False
        for other in aggregated:
            if other is point:
                continue
            # other dominates point if it has <= compression AND >= success
            # (with at least one strict inequality)
            better_compression = other["compression_ratio"] <= point["compression_ratio"]
            better_success = other["success_rate"] >= point["success_rate"]
            strictly_better = other["compression_ratio"] < point["compression_ratio"] or other["success_rate"] > point["success_rate"]

            if better_compression and better_success and strictly_better:
                dominated = True
                break

        if not dominated:
            frontier.append(point)

    # Sort frontier by compression ratio (ascending = most compressed first)
    frontier.sort(key=lambda p: p["compression_ratio"])
    return frontier


def find_knee(frontier: list[dict]) -> dict | None:
    """Find the 'knee' of the Pareto frontier.

    The knee is where the slope of the success-vs-compression tradeoff
    changes most sharply -- below this point, compression causes
    disproportionate success loss.

    Uses the maximum curvature heuristic on the (compression_ratio,
    success_rate) curve.
    """
    if len(frontier) < 3:
        return frontier[-1] if frontier else None

    max_curvature = -1.0
    knee_point = frontier[1]  # default to middle

    for i in range(1, len(frontier) - 1):
        prev_point = frontier[i - 1]
        curr = frontier[i]
        next_point = frontier[i + 1]

        # Approximate curvature as the angle change
        dx1 = curr["compression_ratio"] - prev_point["compression_ratio"]
        dy1 = curr["success_rate"] - prev_point["success_rate"]
        dx2 = next_point["compression_ratio"] - curr["compression_ratio"]
        dy2 = next_point["success_rate"] - curr["success_rate"]

        # Cross product magnitude gives a measure of curvature
        cross = abs(dx1 * dy2 - dx2 * dy1)
        if cross > max_curvature:
            max_curvature = cross
            knee_point = curr

    return knee_point


# ---------------------------------------------------------------------------
# Per-complexity breakdown
# ---------------------------------------------------------------------------


def _breakdown_by_complexity(report: dict) -> dict[str, dict[str, dict]]:
    """Break down success rates by task complexity and variant.

    Returns {complexity: {variant: {success_rate, count}}}.
    """
    data: dict[str, dict[str, list[bool]]] = {}

    for result in report.get("results", []):
        variant = result.get("variant", "unknown")
        complexity = result.get("metadata", {}).get("complexity", "unknown")
        success = result.get("success", False)

        data.setdefault(complexity, {}).setdefault(variant, []).append(success)

    breakdown: dict[str, dict[str, dict]] = {}
    for complexity, variants in sorted(data.items()):
        breakdown[complexity] = {}
        for variant, successes in sorted(variants.items()):
            total = len(successes)
            passed = sum(successes)
            breakdown[complexity][variant] = {
                "success_rate": round(passed / total, 4) if total > 0 else 0.0,
                "count": total,
            }

    return breakdown


# ---------------------------------------------------------------------------
# Output
# ---------------------------------------------------------------------------


def _print_table(aggregated: list[dict], frontier: list[dict], knee: dict | None) -> None:
    """Print a human-readable summary table."""
    frontier_names = {p["variant"] for p in frontier}
    knee_name = knee["variant"] if knee else ""

    print("\n" + "=" * 90)
    print("COMPACTION-EFFICIENCY RESULTS")
    print("=" * 90)

    header = f"{'Strategy':<30} {'Compression':>12} {'Success%':>9} {'Avg Tokens':>11} {'Overhead':>10} {'Pareto':>7}"
    print(f"\n{header}")
    print("-" * 90)

    for row in aggregated:
        pareto_mark = ""
        if row["variant"] in frontier_names:
            pareto_mark = " *"
        if row["variant"] == knee_name:
            pareto_mark = " **"

        print(
            f"{row['variant']:<30} {row['compression_ratio']:>11.2f} "
            f"{row['success_rate']:>8.1%} {row['avg_tokens']:>11,} "
            f"{row['compaction_overhead_ms']:>8}ms{pareto_mark:>7}"
        )

    print("\n* = Pareto frontier   ** = knee point")

    if knee:
        print(f"\nKnee: {knee['variant']} (compression={knee['compression_ratio']:.2f}, success={knee['success_rate']:.1%})")

    print()


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main() -> None:
    parser = argparse.ArgumentParser(description="Analyze compaction-efficiency results")
    parser.add_argument("report", type=Path, help="Path to report.json")
    parser.add_argument("-o", "--output", type=Path, help="Write Pareto analysis to JSON file")
    parser.add_argument("--json", action="store_true", help="Output JSON to stdout")
    args = parser.parse_args()

    report = json.loads(args.report.read_text())

    aggregated = _aggregate_by_variant(report)
    frontier = compute_pareto_frontier(aggregated)
    knee = find_knee(frontier)
    complexity_breakdown = _breakdown_by_complexity(report)

    analysis = {
        "aggregated": aggregated,
        "pareto_frontier": frontier,
        "knee": knee,
        "by_complexity": complexity_breakdown,
    }

    if args.json or args.output:
        output_text = json.dumps(analysis, indent=2, ensure_ascii=False) + "\n"
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(output_text)
            print(f"Analysis written to {args.output}", file=sys.stderr)
        else:
            print(output_text)
    else:
        _print_table(aggregated, frontier, knee)


if __name__ == "__main__":
    main()
