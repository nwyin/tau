#!/usr/bin/env python3
"""Runner + safety scorecard for the fuzzy-match false-positive audit.

Imports matchers from fuzzy-match/matchers.py and evaluates each against
the adversarial corpus.  For every (matcher, case) pair, classifies the
outcome as one of:

- correct:           matched the intended target block
- wrong-location:    matched a DIFFERENT block (the critical failure)
- rejected:          no match found (safe — model retries)
- ambiguous-rejected: multiple matches, none uniquely chosen

Key metric: wrong-location rate per strategy.
Threshold: >1% wrong-location = unsafe for production.
Safety ratio = (correct + rejected) / total.  Target: >99%.

Usage:
    python run.py corpus/adversarial.json
    python run.py corpus/adversarial.json --matchers exact normalized levenshtein-92
    python run.py corpus/adversarial.json --json -o results/audit.json
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from collections import defaultdict
from pathlib import Path

# Import matchers from sibling benchmark
sys.path.insert(0, str(Path(__file__).parent.parent / "fuzzy-match"))
from matchers import MATCHERS, Match  # noqa: E402


# ---------------------------------------------------------------------------
# Evaluation
# ---------------------------------------------------------------------------


def _match_overlaps_candidate(match: Match, candidate: dict, file_content: str) -> bool:
    """Check whether a Match object corresponds to a candidate block.

    We compare by checking if the matched text overlaps the candidate's
    line range.  This is more robust than exact text comparison since
    matchers may return slightly different boundaries.
    """
    lines = file_content.split("\n")
    cand_start = candidate["start_line"]
    cand_end = candidate["end_line"]
    cand_text = "\n".join(lines[cand_start:cand_end])

    # Primary check: matched text is similar to candidate text
    if match.matched_text.strip() == cand_text.strip():
        return True

    # Secondary: byte-offset overlap
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

    # Consider it a match if >50% overlap with either span
    return overlap_len > 0.5 * min(match_len, cand_len)


def evaluate_case(
    matcher_name: str,
    matcher_fn: callable,
    case: dict,
) -> dict:
    """Run a single matcher against a single adversarial case.

    Returns a result dict with the outcome classification.
    """
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
        # Multiple matches — this is inherently ambiguous.
        # Check if any match lands on the target.
        target_cand = all_candidates[target_index]
        has_target = any(_match_overlaps_candidate(m, target_cand, file_content) for m in matches)
        outcome = "ambiguous-rejected" if has_target else "wrong-location"
    else:
        # Exactly one match — did it pick the right candidate?
        the_match = matches[0]
        target_cand = all_candidates[target_index]

        if _match_overlaps_candidate(the_match, target_cand, file_content):
            outcome = "correct"
        else:
            # Check if it matched any OTHER candidate (wrong-location)
            # vs something completely unrelated
            hit_other = any(_match_overlaps_candidate(the_match, c, file_content) for i, c in enumerate(all_candidates) if i != target_index)
            outcome = "wrong-location" if hit_other else "wrong-location"

    return {
        "case_id": case["id"],
        "category": case["category"],
        "matcher": matcher_name,
        "outcome": outcome,
        "n_matches": len(matches),
        "confidence": round(matches[0].confidence, 4) if matches else None,
        "elapsed_us": round(elapsed_us, 1),
    }


# ---------------------------------------------------------------------------
# Runner
# ---------------------------------------------------------------------------


def run_audit(corpus: list[dict], matcher_names: list[str] | None = None) -> list[dict]:
    """Run all requested matchers against the full adversarial corpus."""
    if matcher_names is None:
        matcher_names = list(MATCHERS.keys())

    results: list[dict] = []
    for mname in matcher_names:
        if mname not in MATCHERS:
            print(f"Warning: unknown matcher '{mname}', skipping", file=sys.stderr)
            continue
        fn = MATCHERS[mname]
        for case in corpus:
            results.append(evaluate_case(mname, fn, case))

    return results


# ---------------------------------------------------------------------------
# Scorecard
# ---------------------------------------------------------------------------


def print_scorecard(results: list[dict]) -> None:
    """Print the safety scorecard to stdout."""
    by_matcher: dict[str, list[dict]] = defaultdict(list)
    for r in results:
        by_matcher[r["matcher"]].append(r)

    print()
    print("=" * 90)
    print("FUZZY FALSE-POSITIVE AUDIT — SAFETY SCORECARD")
    print("=" * 90)

    header = f"{'Matcher':<25} {'Cases':>6} {'Correct':>8} {'Wrong':>8} {'Rejected':>9} {'Ambig':>8} {'Safety%':>9}"
    print(f"\n{header}")
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

    # Per-category breakdown
    categories = sorted({r["category"] for r in results})
    if categories:
        print(f"\n{'Per-category wrong-location counts':}")
        cat_header = f"{'Category':<20}"
        active_matchers = [m for m in MATCHERS if m in by_matcher]
        for mname in active_matchers:
            cat_header += f" {mname:>15}"
        print(cat_header)
        print("-" * (20 + 16 * len(active_matchers)))

        for category in categories:
            row = f"{category:<20}"
            for mname in active_matchers:
                cat_cases = [r for r in by_matcher[mname] if r["category"] == category]
                if not cat_cases:
                    row += f" {'--':>15}"
                    continue
                n_cat = len(cat_cases)
                wrong_cat = sum(1 for r in cat_cases if r["outcome"] == "wrong-location")
                correct_cat = sum(1 for r in cat_cases if r["outcome"] == "correct")
                row += f" {correct_cat}/{n_cat} ({wrong_cat}w)"
            print(row)

    # Timing
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

    print()


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main() -> None:
    parser = argparse.ArgumentParser(description="Run fuzzy false-positive audit")
    parser.add_argument("corpus", type=Path, help="Path to adversarial corpus JSON")
    parser.add_argument("--matchers", nargs="+", help="Specific matchers to run (default: all)")
    parser.add_argument("--json", action="store_true", help="Output detailed results as JSON")
    parser.add_argument("-o", "--output", type=Path, help="Write results to file")
    args = parser.parse_args()

    corpus = json.loads(args.corpus.read_text())
    print(f"Loaded {len(corpus)} adversarial cases from {args.corpus}", file=sys.stderr)

    results = run_audit(corpus, matcher_names=args.matchers)

    if args.json or args.output:
        output = json.dumps(results, indent=2)
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(output)
            print(f"Results written to {args.output}", file=sys.stderr)
        else:
            print(output)
    else:
        print_scorecard(results)


if __name__ == "__main__":
    main()
