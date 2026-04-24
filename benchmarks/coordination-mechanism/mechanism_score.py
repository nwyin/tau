"""Trace-first scoring for the coordination-mechanism benchmark."""

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
    normalize_scaffold,
    score_coordination_from_state,
    scaffold_hash,
)


def score_trace(
    events: list[dict[str, Any]],
    output_text: str,
    variant_name: str,
    expectations: CoordinationExpectations,
    expected_scaffold: str,
) -> dict[str, Any]:
    """Score a mechanism-benchmark run using trace evidence."""

    del output_text  # Mechanism benchmark scores the runner-owned artifact, not closing prose.

    state = collect_trace_state(events, expectations)
    final_doc_written = expectations.final_doc in state["document_contents"]
    final_doc_content = state["document_contents"].get(expectations.final_doc, "")
    score = score_coordination_from_state(state, final_doc_content, variant_name, expectations)

    top_level_tool_starts = state["top_level_tool_starts"]
    top_level_tool_names = [str(event.get("tool_name", "?")) for event in top_level_tool_starts]
    top_level_py_repl = [event for event in top_level_tool_starts if event.get("tool_name") == "py_repl"]
    top_level_non_py_repl_tools = [
        str(event.get("tool_name", "?")) for event in top_level_tool_starts if event.get("tool_name") != "py_repl"
    ]

    expected_hash = scaffold_hash(expected_scaffold)
    observed_code = ""
    if len(top_level_py_repl) == 1:
        args = top_level_py_repl[0].get("args", {})
        if isinstance(args, dict):
            observed_code = args.get("code", "") or ""
    observed_hash = scaffold_hash(observed_code) if observed_code else None

    scaffold_fidelity_success = bool(
        len(top_level_tool_starts) == 1
        and len(top_level_py_repl) == 1
        and not top_level_non_py_repl_tools
        and observed_hash == expected_hash
    )

    coordination_success = bool(
        scaffold_fidelity_success
        and score["mechanism_success"]
        and score["timing_success"]
        and final_doc_written
    )

    fail_reasons: list[str] = []
    if not scaffold_fidelity_success:
        if len(top_level_tool_starts) != 1:
            fail_reasons.append(
                f"expected exactly one top-level tool call, saw {len(top_level_tool_starts)}"
            )
        if len(top_level_py_repl) != 1:
            fail_reasons.append(
                f"expected exactly one top-level py_repl call, saw {len(top_level_py_repl)}"
            )
        if top_level_non_py_repl_tools:
            fail_reasons.append(
                "unexpected top-level tools: " + ", ".join(sorted(set(top_level_non_py_repl_tools)))
            )
        if observed_hash != expected_hash:
            fail_reasons.append("executed py_repl scaffold did not match the runner-owned scaffold")
    if not score["mechanism_success"] or not score["timing_success"]:
        fail_reasons.append(score["success_reason"])
    if not final_doc_written:
        fail_reasons.append(f"runner-owned final doc `{expectations.final_doc}` was not written")

    score.update(
        {
            "coordination_success": coordination_success,
            "success_reason": "ok"
            if coordination_success
            else "; ".join(part for part in fail_reasons if part and part != "ok"),
            "trace_event_count": len(events),
            "scaffold_fidelity_success": scaffold_fidelity_success,
            "top_level_tool_count": len(top_level_tool_starts),
            "top_level_tool_names": top_level_tool_names,
            "top_level_py_repl_count": len(top_level_py_repl),
            "top_level_non_py_repl_tools": top_level_non_py_repl_tools,
            "expected_scaffold_hash": expected_hash,
            "observed_scaffold_hash": observed_hash,
            "observed_scaffold_normalized": normalize_scaffold(observed_code) if observed_code else "",
            "final_doc": expectations.final_doc,
            "final_doc_written": final_doc_written,
            "final_doc_length": len(final_doc_content),
            "final_doc_has_both_markers": score["content_has_both_markers"],
        }
    )
    return score


def score_from_trace_file(
    trace_path: Path,
    output_text: str,
    variant_name: str,
    expectations: CoordinationExpectations,
    expected_scaffold: str,
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
            "scaffold_fidelity_success": False,
            "top_level_tool_count": 0,
            "top_level_tool_names": [],
            "top_level_py_repl_count": 0,
            "top_level_non_py_repl_tools": [],
            "expected_scaffold_hash": scaffold_hash(expected_scaffold),
            "observed_scaffold_hash": None,
            "observed_scaffold_normalized": "",
            "final_doc": expectations.final_doc,
            "final_doc_written": False,
            "final_doc_length": 0,
            "final_doc_has_both_markers": False,
        }
    return score_trace(events, output_text, variant_name, expectations, expected_scaffold)
