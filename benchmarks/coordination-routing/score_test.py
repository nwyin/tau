from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from score import CoordinationExpectations, score_trace


EXPECTATIONS = CoordinationExpectations(
    pro_alias="position-for",
    con_alias="position-against",
    critic_alias="critic",
    pro_doc="pro_case_notes",
    con_doc="con_case_notes",
    pro_markers=["PRO_ANCHOR_SOLAR_17"],
    con_markers=["CON_ANCHOR_LEAKAGE_29"],
)


def _doc_tool_start(ts: str, thread_alias: str | None, operation: str, name: str) -> dict:
    return {
        "event": "tool_start",
        "ts": ts,
        "tool_name": "document",
        "thread_alias": thread_alias,
        "args": {
            "operation": operation,
            "name": name,
        },
    }


def _thread_start(ts: str, alias: str) -> dict:
    return {"event": "thread_start", "ts": ts, "alias": alias}


def _thread_end(ts: str, alias: str) -> dict:
    return {"event": "thread_end", "ts": ts, "alias": alias}


def test_no_coordination_fails_even_when_docs_exist() -> None:
    events = [
        _thread_start("2026-04-05T10:00:00+00:00", "position-for"),
        _thread_start("2026-04-05T10:00:00+00:00", "position-against"),
        _thread_start("2026-04-05T10:00:00+00:00", "critic"),
        _doc_tool_start("2026-04-05T10:00:10+00:00", "position-for", "write", "pro_case_notes"),
        _doc_tool_start("2026-04-05T10:00:12+00:00", "position-against", "write", "con_case_notes"),
        _thread_end("2026-04-05T10:00:20+00:00", "critic"),
    ]
    output = "Final synthesis with PRO_ANCHOR_SOLAR_17 and CON_ANCHOR_LEAKAGE_29"
    score = score_trace(events, output, "naive-parallel", EXPECTATIONS)
    assert score["coordination_success"] is False
    assert score["observed_coordination"] is False
    assert "no coordination mechanism observed" in score["success_reason"]


def test_staged_pipeline_passes_with_episode_injection() -> None:
    events = [
        _thread_start("2026-04-05T10:00:00+00:00", "position-for"),
        _thread_start("2026-04-05T10:00:00+00:00", "position-against"),
        _doc_tool_start("2026-04-05T10:00:04+00:00", "position-for", "write", "pro_case_notes"),
        _doc_tool_start("2026-04-05T10:00:05+00:00", "position-against", "write", "con_case_notes"),
        {
            "event": "episode_inject",
            "ts": "2026-04-05T10:00:06+00:00",
            "source_aliases": ["position-for", "position-against"],
            "target_alias": "critic",
        },
        _thread_start("2026-04-05T10:00:06+00:00", "critic"),
        _thread_end("2026-04-05T10:00:09+00:00", "critic"),
    ]
    output = "Synthesis cites PRO_ANCHOR_SOLAR_17 and CON_ANCHOR_LEAKAGE_29"
    score = score_trace(events, output, "staged-pipeline", EXPECTATIONS)
    assert score["coordination_success"] is True
    assert score["episode_inject_has_both_sources"] is True


def test_document_polling_passes_when_critic_reads_after_writes() -> None:
    events = [
        _thread_start("2026-04-05T10:00:00+00:00", "position-for"),
        _thread_start("2026-04-05T10:00:00+00:00", "position-against"),
        _thread_start("2026-04-05T10:00:00+00:00", "critic"),
        _doc_tool_start("2026-04-05T10:00:04+00:00", "position-for", "write", "pro_case_notes"),
        _doc_tool_start("2026-04-05T10:00:05+00:00", "position-against", "write", "con_case_notes"),
        _doc_tool_start("2026-04-05T10:00:06+00:00", "critic", "read", "pro_case_notes"),
        _doc_tool_start("2026-04-05T10:00:07+00:00", "critic", "read", "con_case_notes"),
        _thread_end("2026-04-05T10:00:10+00:00", "critic"),
    ]
    output = "Synthesis uses PRO_ANCHOR_SOLAR_17 and CON_ANCHOR_LEAKAGE_29"
    score = score_trace(events, output, "document-polling", EXPECTATIONS)
    assert score["coordination_success"] is True
    assert score["critic_doc_reads_after_required_writes"] == 2


def test_coordination_can_pass_without_exact_anchor_tokens() -> None:
    events = [
        _thread_start("2026-04-05T10:00:00+00:00", "position-for"),
        _thread_start("2026-04-05T10:00:00+00:00", "position-against"),
        _doc_tool_start("2026-04-05T10:00:04+00:00", "position-for", "write", "pro_case_notes"),
        _doc_tool_start("2026-04-05T10:00:05+00:00", "position-against", "write", "con_case_notes"),
        {
            "event": "episode_inject",
            "ts": "2026-04-05T10:00:06+00:00",
            "source_aliases": ["position-for", "position-against"],
            "target_alias": "critic",
        },
        _thread_start("2026-04-05T10:00:06+00:00", "critic"),
        _thread_end("2026-04-05T10:00:09+00:00", "critic"),
    ]
    # Intentionally omit exact marker tokens from the final text.
    output = "Final synthesis balances payroll-tax relief against leakage risk."
    score = score_trace(events, output, "staged-pipeline", EXPECTATIONS)
    assert score["coordination_success"] is True
    assert score["content_has_both_markers"] is False


def test_orchestrator_reads_do_not_count_as_critic_reads() -> None:
    events = [
        _thread_start("2026-04-05T10:00:00+00:00", "position-for"),
        _thread_start("2026-04-05T10:00:00+00:00", "position-against"),
        _thread_start("2026-04-05T10:00:00+00:00", "critic"),
        _doc_tool_start("2026-04-05T10:00:04+00:00", "position-for", "write", "pro_case_notes"),
        _doc_tool_start("2026-04-05T10:00:05+00:00", "position-against", "write", "con_case_notes"),
        _doc_tool_start("2026-04-05T10:00:06+00:00", None, "read", "pro_case_notes"),
        _doc_tool_start("2026-04-05T10:00:06+00:00", None, "read", "con_case_notes"),
        _thread_end("2026-04-05T10:00:08+00:00", "critic"),
    ]
    output = "Synthesis uses PRO_ANCHOR_SOLAR_17 and CON_ANCHOR_LEAKAGE_29"
    score = score_trace(events, output, "document-polling", EXPECTATIONS)
    assert score["coordination_success"] is False
    assert score["critic_doc_reads_required"] == 0
    assert "critic did not read both required docs after they were written" in score["success_reason"]
