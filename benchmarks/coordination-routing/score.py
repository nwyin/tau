"""Trace-first scoring for the coordination-routing autonomy benchmark."""

from __future__ import annotations

import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.coordination import (
    CoordinationExpectations,
    collect_trace_state,
    expected_coordination_mechanism,
    load_trace_events,
    score_coordination_from_state,
)


def score_trace(
    events: list[dict[str, Any]],
    output_text: str,
    variant_name: str,
    expectations: CoordinationExpectations,
) -> dict[str, Any]:
    """Score a run using trace evidence and final output anchors."""

    state = collect_trace_state(events, expectations)
    score = score_coordination_from_state(state, output_text, variant_name, expectations)

    requested_shape_followed = _requested_shape_followed(score, variant_name)
    self_corrected_to_other_shape = _self_corrected_shape(score, variant_name)
    variant_escape = not requested_shape_followed

    score.update(
        {
            "trace_event_count": len(events),
            "requested_shape_followed": requested_shape_followed,
            "variant_escape": variant_escape,
            "self_corrected_to_other_shape": self_corrected_to_other_shape,
        }
    )
    return score


def score_from_trace_file(
    trace_path: Path,
    output_text: str,
    variant_name: str,
    expectations: CoordinationExpectations,
) -> dict[str, Any]:
    """Load and score trace events from *trace_path*."""
    events = load_trace_events(trace_path)
    if not events:
        return {
            "coordination_success": False,
            "mechanism_success": False,
            "timing_success": False,
            "synthesis_success": False,
            "success_reason": f"trace missing or empty: {trace_path}",
            "trace_event_count": 0,
            "required_docs_written": False,
            "expected_mechanism": expected_coordination_mechanism(variant_name),
            "episode_inject_count_to_critic": 0,
            "episode_inject_has_both_sources": False,
            "critic_doc_reads_total": 0,
            "critic_doc_reads_required": 0,
            "critic_doc_reads_after_required_writes": 0,
            "critic_docs_read": [],
            "critic_docs_read_after_write": [],
            "citations_by_critic": 0,
            "content_has_pro_marker": False,
            "content_has_con_marker": False,
            "content_has_both_markers": False,
            "observed_coordination": False,
            "critic_started_after_required_writes": None,
            "critic_ended_after_required_writes": None,
            "critic_finished_before_required_writes": None,
            "pro_write_ts": None,
            "con_write_ts": None,
            "critic_start_ts": None,
            "critic_end_ts": None,
            "requested_shape_followed": False,
            "variant_escape": False,
            "self_corrected_to_other_shape": None,
        }
    return score_trace(events, output_text, variant_name, expectations)


def _requested_shape_followed(score: dict[str, Any], variant_name: str) -> bool:
    if variant_name == "staged-pipeline":
        return bool(
            score["episode_inject_has_both_sources"]
            and score["critic_started_after_required_writes"] is True
        )
    if variant_name == "document-polling":
        return bool(
            score["episode_inject_has_both_sources"] is False
            and score["critic_started_after_required_writes"] is not True
            and score["critic_doc_reads_after_required_writes"] >= 2
        )
    if variant_name == "prompt-only-parallel":
        return bool(
            score["episode_inject_has_both_sources"] is False
            and score["critic_started_after_required_writes"] is not True
            and score["critic_doc_reads_after_required_writes"] >= 2
        )
    return bool(
        score["episode_inject_has_both_sources"] is False
        and score["critic_started_after_required_writes"] is not True
    )


def _self_corrected_shape(score: dict[str, Any], variant_name: str) -> str | None:
    if _requested_shape_followed(score, variant_name):
        return None

    if score["episode_inject_has_both_sources"] and score["critic_started_after_required_writes"] is True:
        return "staged-pipeline"

    if score["critic_doc_reads_after_required_writes"] >= 2 and score["critic_started_after_required_writes"] is not True:
        return "document-polling"

    return None
