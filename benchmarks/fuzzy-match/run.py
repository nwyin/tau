#!/usr/bin/env python3
"""Run fuzzy match benchmark: evaluate matching strategies against a corpus.

Usage:
    # Generate corpus first
    python generate_corpus.py ../../coding-agent/src -o corpus/synthetic.json

    # Run all matchers against corpus
    python run.py corpus/synthetic.json

    # Run specific matchers
    python run.py corpus/synthetic.json --matchers exact normalized levenshtein-92

    # Output detailed results as JSON
    python run.py corpus/synthetic.json --json -o results.json
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from collections import defaultdict
from pathlib import Path

from matchers import MATCHERS


def evaluate_case(
    matcher_name: str,
    matcher_fn: callable,
    case: dict,
) -> dict:
    """Run a single matcher against a single test case."""
    file_content = case["file_content"]
    old_string = case["old_string"]
    ground_truth = case["ground_truth"]

    start = time.perf_counter()
    matches = matcher_fn(file_content, old_string)
    elapsed_us = (time.perf_counter() - start) * 1_000_000

    gt_text = ground_truth["matched_text"]

    if len(matches) == 0:
        outcome = "missed"
    elif len(matches) == 1:
        if matches[0].matched_text == gt_text:
            outcome = "correct"
        else:
            outcome = "wrong_location"
    else:
        # Multiple matches — check if any is correct
        correct_matches = [m for m in matches if m.matched_text == gt_text]
        if len(correct_matches) == 1 and len(matches) == 1:
            outcome = "correct"
        elif correct_matches:
            outcome = "ambiguous_with_correct"
        else:
            outcome = "wrong_location"

    return {
        "case_id": case["id"],
        "category": case["category"],
        "matcher": matcher_name,
        "outcome": outcome,
        "n_matches": len(matches),
        "confidence": matches[0].confidence if matches else None,
        "elapsed_us": round(elapsed_us, 1),
    }


def run_benchmark(corpus: list[dict], matcher_names: list[str] | None = None) -> list[dict]:
    """Run all matchers against all corpus cases."""
    if matcher_names is None:
        matcher_names = list(MATCHERS.keys())

    results = []
    for matcher_name in matcher_names:
        if matcher_name not in MATCHERS:
            print(f"Warning: unknown matcher '{matcher_name}', skipping", file=sys.stderr)
            continue

        matcher_fn = MATCHERS[matcher_name]
        for case in corpus:
            result = evaluate_case(matcher_name, matcher_fn, case)
            results.append(result)

    return results


def print_scorecard(results: list[dict]):
    """Print a summary scorecard to stdout."""
    # Group by matcher
    by_matcher: dict[str, list[dict]] = defaultdict(list)
    for r in results:
        by_matcher[r["matcher"]].append(r)

    print("\n" + "=" * 80)
    print("FUZZY MATCH BENCHMARK — SCORECARD")
    print("=" * 80)

    # Overall summary
    print(f"\n{'Matcher':<25} {'Cases':>6} {'Correct':>8} {'Missed':>8} {'Wrong':>8} {'Ambig':>8} {'TP%':>7} {'FP%':>7}")
    print("-" * 80)

    for matcher_name in MATCHERS:
        if matcher_name not in by_matcher:
            continue
        cases = by_matcher[matcher_name]
        n = len(cases)
        correct = sum(1 for r in cases if r["outcome"] == "correct")
        missed = sum(1 for r in cases if r["outcome"] == "missed")
        wrong = sum(1 for r in cases if r["outcome"] == "wrong_location")
        ambig = sum(1 for r in cases if r["outcome"] == "ambiguous_with_correct")
        tp_rate = correct / n * 100 if n else 0
        fp_rate = wrong / n * 100 if n else 0

        print(f"{matcher_name:<25} {n:>6} {correct:>8} {missed:>8} {wrong:>8} {ambig:>8} {tp_rate:>6.1f}% {fp_rate:>6.1f}%")

    # Per-category breakdown for each matcher
    categories = sorted({r["category"] for r in results})

    print(f"\n{'Category breakdown':}")
    print(f"\n{'Category':<20}", end="")
    for matcher_name in MATCHERS:
        if matcher_name in by_matcher:
            print(f" {matcher_name:>15}", end="")
    print()
    print("-" * (20 + 16 * len(by_matcher)))

    for category in categories:
        print(f"{category:<20}", end="")
        for matcher_name in MATCHERS:
            if matcher_name not in by_matcher:
                continue
            cat_cases = [r for r in by_matcher[matcher_name] if r["category"] == category]
            if not cat_cases:
                print(f" {'—':>15}", end="")
                continue
            correct = sum(1 for r in cat_cases if r["outcome"] == "correct")
            wrong = sum(1 for r in cat_cases if r["outcome"] == "wrong_location")
            n = len(cat_cases)
            print(f" {correct}/{n} ({wrong}fp)", end="")
        print()

    # Timing
    print(f"\n{'Timing (μs per case)':}")
    print(f"{'Matcher':<25} {'Mean':>10} {'P50':>10} {'P99':>10}")
    print("-" * 55)
    for matcher_name in MATCHERS:
        if matcher_name not in by_matcher:
            continue
        times = sorted(r["elapsed_us"] for r in by_matcher[matcher_name])
        mean = sum(times) / len(times)
        p50 = times[len(times) // 2]
        p99 = times[int(len(times) * 0.99)]
        print(f"{matcher_name:<25} {mean:>10.1f} {p50:>10.1f} {p99:>10.1f}")

    print()


def main():
    parser = argparse.ArgumentParser(description="Run fuzzy match benchmark")
    parser.add_argument("corpus", type=Path, help="Path to corpus JSON file")
    parser.add_argument("--matchers", nargs="+", help="Specific matchers to run (default: all)")
    parser.add_argument("--json", action="store_true", help="Output detailed results as JSON")
    parser.add_argument("-o", "--output", type=Path, help="Write results to file")
    args = parser.parse_args()

    corpus = json.loads(args.corpus.read_text())
    print(f"Loaded {len(corpus)} test cases from {args.corpus}", file=sys.stderr)

    results = run_benchmark(corpus, matcher_names=args.matchers)

    if args.json or args.output:
        output = json.dumps(results, indent=2)
        if args.output:
            args.output.write_text(output)
            print(f"Results written to {args.output}", file=sys.stderr)
        else:
            print(output)
    else:
        print_scorecard(results)


if __name__ == "__main__":
    main()
