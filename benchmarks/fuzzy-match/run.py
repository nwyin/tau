#!/usr/bin/env python3
"""Run fuzzy match benchmark: evaluate matching strategies against a corpus.

Auto-detects corpus type (accuracy vs adversarial) from ground_truth format
and shows the appropriate scorecard.

Usage:
    # Run accuracy benchmark
    python run.py corpus/synthetic.json

    # Run adversarial safety audit
    python run.py corpus/adversarial.json

    # Run both
    python run.py corpus/synthetic.json corpus/adversarial.json

    # Specific matchers
    python run.py corpus/synthetic.json --matchers exact normalized trimmed-cascade

    # JSON output
    python run.py corpus/synthetic.json --json -o results/report.json
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from collections import defaultdict
from pathlib import Path

from matchers import MATCHERS, Match


# ---------------------------------------------------------------------------
# Corpus type detection
# ---------------------------------------------------------------------------


def detect_corpus_type(corpus: list[dict]) -> str:
    """Detect whether corpus is accuracy or adversarial from ground_truth shape."""
    if not corpus:
        return "accuracy"
    gt = corpus[0].get("ground_truth", {})
    if "all_candidates" in gt:
        return "adversarial"
    return "accuracy"


# ---------------------------------------------------------------------------
# Evaluation — accuracy corpus
# ---------------------------------------------------------------------------


def evaluate_accuracy_case(matcher_name: str, matcher_fn: callable, case: dict) -> dict:
    file_content = case["file_content"]
    old_string = case["old_string"]
    gt_text = case["ground_truth"]["matched_text"]

    start = time.perf_counter()
    matches = matcher_fn(file_content, old_string)
    elapsed_us = (time.perf_counter() - start) * 1_000_000

    if len(matches) == 0:
        outcome = "missed"
    elif len(matches) == 1:
        outcome = "correct" if matches[0].matched_text == gt_text else "wrong_location"
    else:
        correct_matches = [m for m in matches if m.matched_text == gt_text]
        if correct_matches:
            outcome = "ambiguous_with_correct"
        else:
            outcome = "wrong_location"

    return {
        "case_id": case["id"],
        "category": case["category"],
        "corpus_type": "accuracy",
        "matcher": matcher_name,
        "outcome": outcome,
        "n_matches": len(matches),
        "confidence": matches[0].confidence if matches else None,
        "elapsed_us": round(elapsed_us, 1),
    }


# ---------------------------------------------------------------------------
# Evaluation — adversarial corpus
# ---------------------------------------------------------------------------


def _match_overlaps_candidate(match: Match, candidate: dict, file_content: str) -> bool:
    lines = file_content.split("\n")
    cand_start = candidate["start_line"]
    cand_end = candidate["end_line"]
    cand_text = "\n".join(lines[cand_start:cand_end])

    if match.matched_text.strip() == cand_text.strip():
        return True

    cand_byte_start = len("\n".join(lines[:cand_start]))
    if cand_start > 0:
        cand_byte_start += 1
    cand_byte_end = cand_byte_start + len(cand_text)

    overlap_start = max(match.start, cand_byte_start)
    overlap_end = min(match.end, cand_byte_end)
    if overlap_end <= overlap_start:
        return False

    overlap_len = overlap_end - overlap_start
    match_len = match.end - match.start
    cand_len = cand_byte_end - cand_byte_start

    return overlap_len > 0.5 * min(match_len, cand_len)


def evaluate_adversarial_case(matcher_name: str, matcher_fn: callable, case: dict) -> dict:
    file_content = case["file_content"]
    old_string = case["old_string"]
    gt = case["ground_truth"]
    target_index = gt["target_index"]
    all_candidates = gt["all_candidates"]

    start = time.perf_counter()
    matches = matcher_fn(file_content, old_string)
    elapsed_us = (time.perf_counter() - start) * 1_000_000

    if len(matches) == 0:
        outcome = "rejected"
    elif len(matches) > 1:
        target_cand = all_candidates[target_index]
        has_target = any(_match_overlaps_candidate(m, target_cand, file_content) for m in matches)
        outcome = "ambiguous-rejected" if has_target else "wrong-location"
    else:
        target_cand = all_candidates[target_index]
        if _match_overlaps_candidate(matches[0], target_cand, file_content):
            outcome = "correct"
        else:
            outcome = "wrong-location"

    return {
        "case_id": case["id"],
        "category": case["category"],
        "corpus_type": "adversarial",
        "matcher": matcher_name,
        "outcome": outcome,
        "n_matches": len(matches),
        "confidence": round(matches[0].confidence, 4) if matches else None,
        "elapsed_us": round(elapsed_us, 1),
    }


# ---------------------------------------------------------------------------
# Runner
# ---------------------------------------------------------------------------


def run_benchmark(corpus: list[dict], matcher_names: list[str] | None = None) -> list[dict]:
    if matcher_names is None:
        matcher_names = list(MATCHERS.keys())

    corpus_type = detect_corpus_type(corpus)
    evaluate_fn = evaluate_accuracy_case if corpus_type == "accuracy" else evaluate_adversarial_case

    results = []
    for mname in matcher_names:
        if mname not in MATCHERS:
            print(f"Warning: unknown matcher '{mname}', skipping", file=sys.stderr)
            continue
        fn = MATCHERS[mname]
        for case in corpus:
            results.append(evaluate_fn(mname, fn, case))

    return results


# ---------------------------------------------------------------------------
# Scorecards
# ---------------------------------------------------------------------------


def _group_by_matcher(results: list[dict]) -> dict[str, list[dict]]:
    by_matcher: dict[str, list[dict]] = defaultdict(list)
    for r in results:
        by_matcher[r["matcher"]].append(r)
    return by_matcher


def _print_timing(by_matcher: dict[str, list[dict]]) -> None:
    print(f"\n{'Timing (us per case)':}")
    print(f"{'Matcher':<25} {'Mean':>10} {'P50':>10} {'P99':>10}")
    print("-" * 55)
    for mname in MATCHERS:
        if mname not in by_matcher:
            continue
        times = sorted(r["elapsed_us"] for r in by_matcher[mname])
        mean = sum(times) / len(times)
        p50 = times[len(times) // 2]
        p99 = times[int(len(times) * 0.99)]
        print(f"{mname:<25} {mean:>10.1f} {p50:>10.1f} {p99:>10.1f}")


def _print_category_breakdown(by_matcher: dict[str, list[dict]], results: list[dict], is_adversarial: bool) -> None:
    categories = sorted({r["category"] for r in results})
    active_matchers = [m for m in MATCHERS if m in by_matcher]

    label = "Per-category wrong-location counts" if is_adversarial else "Category breakdown"
    print(f"\n{label}:")
    print(f"\n{'Category':<20}", end="")
    for mname in active_matchers:
        print(f" {mname:>15}", end="")
    print()
    print("-" * (20 + 16 * len(active_matchers)))

    for category in categories:
        print(f"{category:<20}", end="")
        for mname in active_matchers:
            cat_cases = [r for r in by_matcher[mname] if r["category"] == category]
            if not cat_cases:
                print(f" {'--':>15}", end="")
                continue
            n = len(cat_cases)
            correct = sum(1 for r in cat_cases if r["outcome"] == "correct")
            if is_adversarial:
                wrong = sum(1 for r in cat_cases if r["outcome"] == "wrong-location")
                print(f" {correct}/{n} ({wrong}w)", end="")
            else:
                wrong = sum(1 for r in cat_cases if r["outcome"] == "wrong_location")
                print(f" {correct}/{n} ({wrong}fp)", end="")
        print()


def print_accuracy_scorecard(results: list[dict]) -> None:
    by_matcher = _group_by_matcher(results)

    print("\n" + "=" * 80)
    print("FUZZY MATCH — ACCURACY SCORECARD")
    print("=" * 80)

    print(f"\n{'Matcher':<25} {'Cases':>6} {'Correct':>8} {'Missed':>8} {'Wrong':>8} {'Ambig':>8} {'TP%':>7} {'FP%':>7}")
    print("-" * 80)

    for mname in MATCHERS:
        if mname not in by_matcher:
            continue
        cases = by_matcher[mname]
        n = len(cases)
        correct = sum(1 for r in cases if r["outcome"] == "correct")
        missed = sum(1 for r in cases if r["outcome"] == "missed")
        wrong = sum(1 for r in cases if r["outcome"] == "wrong_location")
        ambig = sum(1 for r in cases if r["outcome"] == "ambiguous_with_correct")
        tp_rate = correct / n * 100 if n else 0
        fp_rate = wrong / n * 100 if n else 0
        print(f"{mname:<25} {n:>6} {correct:>8} {missed:>8} {wrong:>8} {ambig:>8} {tp_rate:>6.1f}% {fp_rate:>6.1f}%")

    _print_category_breakdown(by_matcher, results, is_adversarial=False)
    _print_timing(by_matcher)
    print()


def print_adversarial_scorecard(results: list[dict]) -> None:
    by_matcher = _group_by_matcher(results)

    print("\n" + "=" * 90)
    print("FUZZY MATCH — SAFETY SCORECARD (adversarial)")
    print("=" * 90)

    print(f"\n{'Matcher':<25} {'Cases':>6} {'Correct':>8} {'Wrong':>8} {'Rejected':>9} {'Ambig':>8} {'Safety%':>9}")
    print("-" * 90)

    for mname in MATCHERS:
        if mname not in by_matcher:
            continue
        cases = by_matcher[mname]
        n = len(cases)
        correct = sum(1 for r in cases if r["outcome"] == "correct")
        wrong = sum(1 for r in cases if r["outcome"] == "wrong-location")
        rejected = sum(1 for r in cases if r["outcome"] == "rejected")
        ambig = sum(1 for r in cases if r["outcome"] == "ambiguous-rejected")

        safe = correct + rejected + ambig
        safety_pct = safe / n * 100 if n else 0
        wrong_pct = wrong / n * 100 if n else 0

        marker = " *** UNSAFE ***" if wrong_pct > 1.0 else ""
        print(f"{mname:<25} {n:>6} {correct:>8} {wrong:>8} {rejected:>9} {ambig:>8} {safety_pct:>8.1f}%{marker}")

    _print_category_breakdown(by_matcher, results, is_adversarial=True)
    _print_timing(by_matcher)
    print()


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main():
    parser = argparse.ArgumentParser(description="Run fuzzy match benchmark")
    parser.add_argument("corpus", nargs="+", type=Path, help="Path(s) to corpus JSON file(s)")
    parser.add_argument("--matchers", nargs="+", help="Specific matchers to run (default: all)")
    parser.add_argument("--json", action="store_true", help="Output detailed results as JSON")
    parser.add_argument("-o", "--output", type=Path, help="Write results to file")
    args = parser.parse_args()

    all_results = []
    for corpus_path in args.corpus:
        corpus = json.loads(corpus_path.read_text())
        corpus_type = detect_corpus_type(corpus)
        print(f"Loaded {len(corpus)} {corpus_type} cases from {corpus_path}", file=sys.stderr)

        results = run_benchmark(corpus, matcher_names=args.matchers)
        all_results.extend(results)

        if not (args.json or args.output):
            if corpus_type == "adversarial":
                print_adversarial_scorecard(results)
            else:
                print_accuracy_scorecard(results)

    if args.json or args.output:
        output = json.dumps(all_results, indent=2)
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(output)
            print(f"Results written to {args.output}", file=sys.stderr)
        else:
            print(output)


if __name__ == "__main__":
    main()
