#!/usr/bin/env python3
"""Recall accuracy scorer for compaction-recall benchmark.

Checks whether model responses to recall questions contain the expected terms
from the planted facts.  Computes per-fact and aggregate recall accuracy, and
detects possible hallucinations (terms not related to any planted fact).

Library usage:
    from score import score_response, score_report

CLI usage:
    python score.py results/report.json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Scoring helpers
# ---------------------------------------------------------------------------


def score_response(
    response_text: str,
    expected_contains: list[str],
) -> dict:
    """Score a single recall response against expected answer terms.

    Returns a dict with:
        - matched: list of terms found (case-insensitive)
        - missed: list of terms not found
        - recall: fraction of expected terms found (0.0 - 1.0)
    """
    lower_response = response_text.lower()
    matched: list[str] = []
    missed: list[str] = []

    for term in expected_contains:
        if term.lower() in lower_response:
            matched.append(term)
        else:
            missed.append(term)

    total = len(expected_contains)
    recall = len(matched) / total if total > 0 else 0.0

    return {
        "matched": matched,
        "missed": missed,
        "recall": recall,
    }


def detect_hallucinations(
    response_text: str,
    all_planted_terms: list[str],
    *,
    known_safe_terms: list[str] | None = None,
) -> list[str]:
    """Detect potential hallucinated terms in a recall response.

    This is a heuristic: we look for technical-looking terms (identifiers,
    file paths, numbers with units) in the response that don't appear in
    any planted fact's expected terms.

    Returns a list of suspicious terms.  Empty list means no hallucination
    detected (which is the desired outcome).
    """
    # Normalize planted terms for lookup
    planted_lower = {t.lower() for t in all_planted_terms}
    if known_safe_terms:
        planted_lower.update(t.lower() for t in known_safe_terms)

    # Simple heuristic: look for identifiers that look like function names,
    # file paths, or error names.  This is intentionally conservative --
    # false negatives are fine; false positives are not.
    import re

    suspicious: list[str] = []

    # Match function-like names: word_word() or word_word
    func_pattern = re.compile(r"\b([a-z_][a-z0-9_]*\(\))\b")
    for m in func_pattern.finditer(response_text.lower()):
        name = m.group(1).rstrip("()")
        if name not in planted_lower and len(name) > 3:
            suspicious.append(m.group(1))

    # Match file paths: word/word.ext
    path_pattern = re.compile(r"\b([\w/]+\.\w{1,4})\b")
    for m in path_pattern.finditer(response_text):
        path = m.group(1)
        if path.lower() not in planted_lower and "/" in path:
            suspicious.append(path)

    return suspicious


# ---------------------------------------------------------------------------
# Report-level scoring
# ---------------------------------------------------------------------------


def score_report(report: dict) -> dict:
    """Score all recall results in a benchmark report.

    Expects report["results"] to be a list of task results, each with
    metadata containing:
        - planted_facts: list of planted fact dicts
        - recall_responses: list of model response strings (parallel to planted_facts)

    Returns aggregate scores.
    """
    all_fact_scores: list[dict] = []
    by_category: dict[str, list[float]] = {}
    by_variant: dict[str, list[float]] = {}
    total_hallucinations = 0

    for result in report.get("results", []):
        meta = result.get("metadata", {})
        planted_facts = meta.get("planted_facts", [])
        recall_responses = meta.get("recall_responses", [])
        variant = result.get("variant", "unknown")

        # Collect all planted terms for hallucination detection
        all_terms: list[str] = []
        for fact in planted_facts:
            all_terms.extend(fact.get("expected_answer_contains", []))

        for i, fact in enumerate(planted_facts):
            if i >= len(recall_responses):
                break

            response = recall_responses[i]
            expected = fact.get("expected_answer_contains", [])
            category = fact.get("category", "unknown")

            fact_score = score_response(response, expected)
            fact_score["category"] = category
            fact_score["variant"] = variant
            fact_score["task_id"] = result.get("task_id", "unknown")
            all_fact_scores.append(fact_score)

            # Aggregate by category
            by_category.setdefault(category, []).append(fact_score["recall"])
            by_variant.setdefault(variant, []).append(fact_score["recall"])

            # Hallucination detection
            hallucinations = detect_hallucinations(response, all_terms)
            fact_score["hallucinations"] = hallucinations
            total_hallucinations += len(hallucinations)

    # Compute aggregates
    all_recalls = [s["recall"] for s in all_fact_scores]
    overall_recall = sum(all_recalls) / len(all_recalls) if all_recalls else 0.0

    category_summary = {cat: sum(vals) / len(vals) for cat, vals in by_category.items()}

    variant_summary = {}
    for var, vals in by_variant.items():
        variant_summary[var] = {
            "recall": sum(vals) / len(vals),
            "count": len(vals),
            "perfect_recall": sum(1 for v in vals if v == 1.0),
        }

    return {
        "overall_recall": round(overall_recall, 4),
        "total_facts_tested": len(all_fact_scores),
        "total_hallucinations": total_hallucinations,
        "hallucination_rate": round(total_hallucinations / len(all_fact_scores), 4) if all_fact_scores else 0.0,
        "by_category": {k: round(v, 4) for k, v in category_summary.items()},
        "by_variant": variant_summary,
        "detailed_scores": all_fact_scores,
    }


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def _print_scorecard(scores: dict) -> None:
    """Print a human-readable scorecard."""
    print("\n" + "=" * 70)
    print("COMPACTION-RECALL SCORECARD")
    print("=" * 70)

    print(f"\nOverall recall:     {scores['overall_recall']:.1%}")
    print(f"Facts tested:       {scores['total_facts_tested']}")
    print(f"Hallucinations:     {scores['total_hallucinations']}")
    print(f"Hallucination rate: {scores['hallucination_rate']:.1%}")

    print(f"\n{'Category':<25} {'Recall':>8}")
    print("-" * 35)
    for cat, recall in sorted(scores["by_category"].items()):
        print(f"{cat:<25} {recall:>7.1%}")

    print(f"\n{'Variant':<25} {'Recall':>8} {'Count':>7} {'Perfect':>8}")
    print("-" * 50)
    for var, info in sorted(scores["by_variant"].items()):
        print(f"{var:<25} {info['recall']:>7.1%} {info['count']:>7} {info['perfect_recall']:>8}")

    print()


def main() -> None:
    parser = argparse.ArgumentParser(description="Score compaction-recall benchmark results")
    parser.add_argument("report", type=Path, help="Path to report JSON file")
    parser.add_argument("--json", action="store_true", help="Output scores as JSON")
    parser.add_argument("-o", "--output", type=Path, help="Write scores to file")
    args = parser.parse_args()

    report = json.loads(args.report.read_text())
    scores = score_report(report)

    if args.json or args.output:
        output_text = json.dumps(scores, indent=2, ensure_ascii=False) + "\n"
        if args.output:
            args.output.write_text(output_text)
            print(f"Scores written to {args.output}", file=sys.stderr)
        else:
            print(output_text)
    else:
        _print_scorecard(scores)


if __name__ == "__main__":
    main()
