#!/usr/bin/env python3
"""Runner for compaction-recall benchmark.

Replays pre-compaction conversation turns via TauSession, triggers compaction
at the specified turn, continues with post-compaction turns, then sends recall
questions and scores the responses.

Usage:
    python run.py fixtures/ \\
        --model claude-sonnet-4-6 \\
        --variants truncation,observation-mask,llm-summary,progressive \\
        -o results/

    python run.py fixtures/recall-001.json \\
        --model claude-sonnet-4-6 \\
        --variants truncation \\
        -o results/
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.config import BenchConfig
from shared.reporter import Reporter
from shared.result import TaskResult
from shared.session import SessionResult, TauSession
from shared.store import ResultStore
from shared.variants import Variant

from score import score_response
from variants import get_variants

# ---------------------------------------------------------------------------
# Core runner logic
# ---------------------------------------------------------------------------


def load_conversations(fixture_path: Path) -> list[dict]:
    """Load conversation fixtures from a file or directory."""
    if fixture_path.is_file():
        data = json.loads(fixture_path.read_text())
        if isinstance(data, list):
            return data
        return [data]

    conversations: list[dict] = []
    for f in sorted(fixture_path.glob("*.json")):
        if f.name == "all.json":
            continue
        data = json.loads(f.read_text())
        if isinstance(data, list):
            conversations.extend(data)
        else:
            conversations.append(data)
    return conversations


def run_single(
    conversation: dict,
    variant: Variant,
    config: BenchConfig,
    run_index: int,
) -> TaskResult:
    """Run a single conversation through compaction and recall.

    Flow:
      1. Replay pre-compaction turns (1 to compaction_trigger_turn)
      2. Trigger compaction via tau's compaction API
      3. Replay post-compaction turns (compaction_trigger_turn+1 to 40)
      4. Send recall questions and capture responses
      5. Score responses
    """
    conv_id = conversation["id"]
    trigger_turn = conversation["compaction_trigger_turn"]
    planted_facts = conversation["planted_facts"]
    messages = conversation["conversation"]

    start_time = time.monotonic()
    total_input_tokens = 0
    total_output_tokens = 0
    total_tool_calls = 0
    turn_count = 0
    recall_responses: list[str] = []

    try:
        # TODO: requires compaction feature in tau
        # The TauSession needs to support:
        #   1. Configuring compaction strategy via variant.tau_config_overrides
        #   2. Triggering compaction at a specific turn (or token threshold)
        #   3. Reporting compaction metrics (tokens before/after, latency)
        with TauSession(
            model=config.model,
            cwd=Path("."),
            timeout=config.timeout,
        ) as session:
            # Phase 1: Replay pre-compaction user turns
            for msg in messages:
                if msg["turn"] > trigger_turn:
                    break
                if msg["role"] != "user":
                    continue

                result: SessionResult = session.send(msg["content"])
                total_input_tokens += result.input_tokens
                total_output_tokens += result.output_tokens
                total_tool_calls += result.tool_calls
                turn_count += 1

            # Phase 2: Trigger compaction
            # TODO: requires compaction feature in tau
            # session.trigger_compaction(
            #     strategy=variant.tau_config_overrides.get("compaction_strategy", "truncation"),
            #     keep_ratio=variant.tau_config_overrides.get("compaction_keep_ratio", 0.5),
            # )

            # Phase 3: Replay post-compaction turns
            for msg in messages:
                if msg["turn"] <= trigger_turn or msg["turn"] > 40:
                    continue
                if msg["role"] != "user":
                    continue

                result = session.send(msg["content"])
                total_input_tokens += result.input_tokens
                total_output_tokens += result.output_tokens
                total_tool_calls += result.tool_calls
                turn_count += 1

            # Phase 4: Send recall questions
            for fact in planted_facts:
                result = session.send(fact["recall_question"])
                total_input_tokens += result.input_tokens
                total_output_tokens += result.output_tokens
                total_tool_calls += result.tool_calls
                turn_count += 1
                recall_responses.append(result.output)

        # Phase 5: Score
        fact_scores: list[dict] = []
        for i, fact in enumerate(planted_facts):
            if i < len(recall_responses):
                s = score_response(recall_responses[i], fact["expected_answer_contains"])
                s["category"] = fact["category"]
                fact_scores.append(s)

        all_recalls = [s["recall"] for s in fact_scores]
        avg_recall = sum(all_recalls) / len(all_recalls) if all_recalls else 0.0

        elapsed_ms = int((time.monotonic() - start_time) * 1000)

        return TaskResult(
            task_id=conv_id,
            variant=variant.name,
            run_index=run_index,
            success=avg_recall >= 0.5,  # success threshold: 50% recall
            wall_clock_ms=elapsed_ms,
            input_tokens=total_input_tokens,
            output_tokens=total_output_tokens,
            turns=turn_count,
            tool_calls=total_tool_calls,
            metadata={
                "recall_accuracy": round(avg_recall, 4),
                "planted_facts": planted_facts,
                "recall_responses": recall_responses,
                "fact_scores": fact_scores,
            },
        )

    except Exception as e:
        elapsed_ms = int((time.monotonic() - start_time) * 1000)
        return TaskResult(
            task_id=conv_id,
            variant=variant.name,
            run_index=run_index,
            success=False,
            wall_clock_ms=elapsed_ms,
            input_tokens=total_input_tokens,
            output_tokens=total_output_tokens,
            turns=turn_count,
            tool_calls=total_tool_calls,
            error=str(e),
            metadata={
                "planted_facts": planted_facts,
                "recall_responses": recall_responses,
            },
        )


# ---------------------------------------------------------------------------
# Main runner
# ---------------------------------------------------------------------------


def run_benchmark(
    fixture_path: Path,
    variants: list[Variant],
    config: BenchConfig,
) -> list[TaskResult]:
    """Run all conversations x variants x runs."""
    conversations = load_conversations(fixture_path)
    results: list[TaskResult] = []

    total = len(conversations) * len(variants) * config.runs_per_task
    completed = 0

    for variant in variants:
        for conv in conversations:
            for run_idx in range(config.runs_per_task):
                completed += 1
                print(
                    f"[{completed}/{total}] {conv['id']} / {variant.name} / run {run_idx + 1}",
                    file=sys.stderr,
                )
                result = run_single(conv, variant, config, run_idx)
                results.append(result)

                recall = result.metadata.get("recall_accuracy", 0)
                status = "PASS" if result.success else "FAIL"
                print(
                    f"  -> {status} recall={recall:.1%} tokens={result.input_tokens + result.output_tokens}",
                    file=sys.stderr,
                )

    return results


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main() -> None:
    parser = argparse.ArgumentParser(description="Run compaction-recall benchmark")
    parser.add_argument("fixtures", type=Path, help="Path to fixtures directory or single JSON file")
    parser.add_argument("--model", default="claude-sonnet-4-6", help="Model to use (default: claude-sonnet-4-6)")
    parser.add_argument("--variants", type=str, default=None, help="Comma-separated variant names (default: all)")
    parser.add_argument("--runs", type=int, default=1, help="Runs per task per variant (default: 1)")
    parser.add_argument("--timeout", type=int, default=120, help="Timeout per task in seconds (default: 120)")
    parser.add_argument("-o", "--output", type=Path, default=Path("results"), help="Output directory (default: results/)")
    parser.add_argument("--json", action="store_true", help="Output JSON to stdout")
    args = parser.parse_args()

    variant_names = args.variants.split(",") if args.variants else None
    variants = get_variants(variant_names)

    config = BenchConfig(
        model=args.model,
        runs_per_task=args.runs,
        timeout=args.timeout,
        output_dir=args.output,
        concurrency=1,  # compaction benchmarks are memory-intensive
    )

    results = run_benchmark(args.fixtures, variants, config)

    reporter = Reporter(benchmark_name="compaction-recall", results=results, config=config)

    if args.json:
        print(reporter.json())
    else:
        args.output.mkdir(parents=True, exist_ok=True)
        reporter.write(args.output)
        print(f"\nResults written to {args.output}/", file=sys.stderr)

        # Also save to the result store
        store = ResultStore(benchmark="compaction-recall")
        report = json.loads(reporter.json())
        run_id = store.save(report)
        print(f"Stored as run: {run_id}", file=sys.stderr)


if __name__ == "__main__":
    main()
