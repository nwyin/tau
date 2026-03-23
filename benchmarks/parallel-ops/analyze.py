"""Scaling analysis for the parallel-ops benchmark.

Reads report.json and computes speedup ratios, scaling tables, and
variant comparisons across file counts.

Usage:
    uv run python analyze.py results/report.json
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import defaultdict
from pathlib import Path


def load_report(path: Path) -> dict:
    """Load a report.json file."""
    with open(path) as f:
        return json.load(f)


def extract_results(report: dict) -> list[dict]:
    """Extract the results list from a report."""
    return report.get("results", [])


def group_results(results: list[dict]) -> dict[tuple[str, int], list[dict]]:
    """Group results by (variant, file_count)."""
    groups: dict[tuple[str, int], list[dict]] = defaultdict(list)
    for r in results:
        variant = r.get("variant", "unknown")
        file_count = r.get("metadata", {}).get("file_count", 0)
        groups[(variant, file_count)].append(r)
    return groups


def compute_stats(results: list[dict]) -> dict:
    """Compute summary statistics for a group of results."""
    n = len(results)
    if n == 0:
        return {"n": 0, "avg_time_ms": 0, "avg_turns": 0, "avg_tokens": 0, "success_rate": 0.0}

    total_time = sum(r.get("wall_clock_ms", 0) for r in results)
    total_turns = sum(r.get("turns", 0) for r in results)
    total_tokens = sum(r.get("input_tokens", 0) + r.get("output_tokens", 0) for r in results)
    successes = sum(1 for r in results if r.get("success", False))

    return {
        "n": n,
        "avg_time_ms": total_time / n,
        "avg_turns": total_turns / n,
        "avg_tokens": total_tokens / n,
        "success_rate": successes / n,
    }


def print_scaling_table(groups: dict[tuple[str, int], list[dict]]) -> None:
    """Print the main scaling analysis table."""
    # Collect all variants and file counts
    variants = sorted({v for v, _ in groups})
    file_counts = sorted({fc for _, fc in groups})

    # Header
    print()
    print("=" * 100)
    print("SCALING ANALYSIS")
    print("=" * 100)
    print()
    print(f"{'Variant':<15} {'Files':>6} {'Runs':>5} {'Avg Time(ms)':>13} {'Avg Turns':>10} {'Avg Tokens':>11} {'Success%':>9}")
    print("-" * 80)

    for variant in variants:
        for fc in file_counts:
            key = (variant, fc)
            if key not in groups:
                continue
            stats = compute_stats(groups[key])
            print(
                f"{variant:<15} {fc:>6} {stats['n']:>5} "
                f"{stats['avg_time_ms']:>13.0f} {stats['avg_turns']:>10.1f} "
                f"{stats['avg_tokens']:>11.0f} {stats['success_rate']:>8.0%}"
            )
        print()


def print_speedup_ratios(groups: dict[tuple[str, int], list[dict]]) -> None:
    """Print speedup ratios comparing parallel and natural to sequential."""
    variants = sorted({v for v, _ in groups})
    file_counts = sorted({fc for _, fc in groups})

    if "sequential" not in variants:
        print("\nNo 'sequential' variant found — cannot compute speedup ratios.")
        return

    print()
    print("=" * 80)
    print("SPEEDUP vs SEQUENTIAL")
    print("=" * 80)
    print()
    print(f"{'Variant':<15} {'Files':>6} {'Time Speedup':>13} {'Turn Reduction':>15} {'Token Delta':>12}")
    print("-" * 70)

    for variant in variants:
        if variant == "sequential":
            continue
        for fc in file_counts:
            seq_key = ("sequential", fc)
            var_key = (variant, fc)
            if seq_key not in groups or var_key not in groups:
                continue
            seq_stats = compute_stats(groups[seq_key])
            var_stats = compute_stats(groups[var_key])

            if seq_stats["avg_time_ms"] > 0:
                time_speedup = seq_stats["avg_time_ms"] / var_stats["avg_time_ms"]
            else:
                time_speedup = 0.0

            if seq_stats["avg_turns"] > 0:
                turn_reduction = 1.0 - (var_stats["avg_turns"] / seq_stats["avg_turns"])
            else:
                turn_reduction = 0.0

            if seq_stats["avg_tokens"] > 0:
                token_delta = (var_stats["avg_tokens"] - seq_stats["avg_tokens"]) / seq_stats["avg_tokens"]
            else:
                token_delta = 0.0

            print(f"{variant:<15} {fc:>6} {time_speedup:>12.2f}x {turn_reduction:>14.0%} {token_delta:>+11.0%}")
        print()


def print_summary(groups: dict[tuple[str, int], list[dict]]) -> None:
    """Print a high-level summary of key findings."""
    variants = sorted({v for v, _ in groups})

    print()
    print("=" * 80)
    print("SUMMARY")
    print("=" * 80)

    # Overall stats per variant
    variant_results: dict[str, list[dict]] = defaultdict(list)
    for (v, _), results in groups.items():
        variant_results[v].extend(results)

    print()
    for variant in variants:
        stats = compute_stats(variant_results[variant])
        print(
            f"  {variant}: {stats['n']} runs, "
            f"avg {stats['avg_time_ms']:.0f}ms, "
            f"avg {stats['avg_turns']:.1f} turns, "
            f"{stats['success_rate']:.0%} success"
        )

    # Win criteria check
    if "sequential" in variant_results and "parallel" in variant_results:
        seq = compute_stats(variant_results["sequential"])
        par = compute_stats(variant_results["parallel"])

        print()
        print("Win criteria (parallel vs sequential):")
        if seq["avg_time_ms"] > 0:
            speedup = (seq["avg_time_ms"] - par["avg_time_ms"]) / seq["avg_time_ms"]
            met = speedup >= 0.20
            print(f"  Wall-clock >= 20% faster: {speedup:.0%} {'PASS' if met else 'FAIL'}")
        if seq["avg_tokens"] > 0:
            overhead = (par["avg_tokens"] - seq["avg_tokens"]) / seq["avg_tokens"]
            met = overhead <= 0.10
            print(f"  Tokens <= 10% more:       {overhead:+.0%} {'PASS' if met else 'FAIL'}")
        if seq["success_rate"] > 0:
            degradation = seq["success_rate"] - par["success_rate"]
            met = degradation <= 0.0
            print(f"  No correctness loss:      {degradation:+.0%} {'PASS' if met else 'FAIL'}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Analyze parallel-ops benchmark results",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("report", type=Path, help="Path to report.json")
    parser.add_argument(
        "--json",
        action="store_true",
        help="Output analysis as JSON instead of table",
    )
    return parser


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()

    report_path: Path = args.report
    if not report_path.exists():
        print(f"Error: report not found: {report_path}", file=sys.stderr)
        sys.exit(1)

    report = load_report(report_path)
    results = extract_results(report)
    if not results:
        print("Error: no results found in report", file=sys.stderr)
        sys.exit(1)

    groups = group_results(results)

    if args.json:
        # Machine-readable output
        analysis: dict = {}
        for (variant, fc), group in groups.items():
            key = f"{variant}_{fc}"
            analysis[key] = compute_stats(group)
        json.dump(analysis, sys.stdout, indent=2)
        print()
    else:
        print("Parallel-Ops Benchmark Analysis")
        print(f"Report: {report_path}")
        print(f"Model: {report.get('config', {}).get('model', 'unknown')}")
        print(f"Total results: {len(results)}")

        print_scaling_table(groups)
        print_speedup_ratios(groups)
        print_summary(groups)


if __name__ == "__main__":
    main()
