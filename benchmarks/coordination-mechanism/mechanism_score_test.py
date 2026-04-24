from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from mechanism_score import CoordinationExpectations, score_trace


EXPECTATIONS = CoordinationExpectations(
    pro_alias="position-for",
    con_alias="position-against",
    critic_alias="critic",
    pro_doc="pro_case_notes",
    con_doc="con_case_notes",
    pro_markers=["PRO_ANCHOR_SOLAR_17"],
    con_markers=["CON_ANCHOR_LEAKAGE_29"],
    pro_task="pro task",
    con_task="con task",
    critic_task="critic task",
    final_doc="final_synthesis",
)

SCAFFOLD = "tau.parallel('a')\nprint('DONE')\n"


def _py_repl_start(code: str) -> dict:
    return {
        "event": "tool_start",
        "ts": "2026-04-05T10:00:00+00:00",
        "tool_name": "py_repl",
        "thread_alias": None,
        "args": {"code": code},
    }


def _doc_op(ts: str, thread_alias: str | None, operation: str, name: str, content: str) -> dict:
    return {
        "event": "document_op",
        "ts": ts,
        "thread_alias": thread_alias,
        "op": operation,
        "name": name,
        "content": content,
    }


def _thread_start(ts: str, alias: str) -> dict:
    return {"event": "thread_start", "ts": ts, "alias": alias}


def _thread_end(ts: str, alias: str) -> dict:
    return {"event": "thread_end", "ts": ts, "alias": alias}


def test_staged_pipeline_requires_scaffold_fidelity_and_runner_owned_final_doc() -> None:
    events = [
        _py_repl_start(SCAFFOLD),
        _thread_start("2026-04-05T10:00:01+00:00", "position-for"),
        _thread_start("2026-04-05T10:00:01+00:00", "position-against"),
        _doc_op(
            "2026-04-05T10:00:04+00:00",
            "position-for",
            "write",
            "pro_case_notes",
            "FOR PRO_ANCHOR_SOLAR_17",
        ),
        _doc_op(
            "2026-04-05T10:00:05+00:00",
            "position-against",
            "write",
            "con_case_notes",
            "AGAINST CON_ANCHOR_LEAKAGE_29",
        ),
        {
            "event": "episode_inject",
            "ts": "2026-04-05T10:00:06+00:00",
            "source_aliases": ["position-for", "position-against"],
            "target_alias": "critic",
        },
        _thread_start("2026-04-05T10:00:06+00:00", "critic"),
        _thread_end("2026-04-05T10:00:10+00:00", "critic"),
        _doc_op(
            "2026-04-05T10:00:11+00:00",
            None,
            "write",
            "final_synthesis",
            "Balanced synthesis PRO_ANCHOR_SOLAR_17 and CON_ANCHOR_LEAKAGE_29",
        ),
    ]

    score = score_trace(events, "ignored", "staged-pipeline", EXPECTATIONS, SCAFFOLD)

    assert score["coordination_success"] is True
    assert score["scaffold_fidelity_success"] is True
    assert score["mechanism_success"] is True
    assert score["timing_success"] is True
    assert score["synthesis_success"] is True
    assert score["final_doc_written"] is True
    assert score["top_level_py_repl_count"] == 1


def test_scaffold_fidelity_fails_on_top_level_escape_and_hash_mismatch() -> None:
    events = [
        _py_repl_start("print('not the right scaffold')"),
        {
            "event": "tool_start",
            "ts": "2026-04-05T10:00:00+00:00",
            "tool_name": "thread",
            "thread_alias": None,
            "args": {"alias": "critic", "task": "bad escape"},
        },
    ]

    score = score_trace(events, "ignored", "naive-parallel", EXPECTATIONS, SCAFFOLD)

    assert score["coordination_success"] is False
    assert score["scaffold_fidelity_success"] is False
    assert score["top_level_tool_count"] == 2
    assert score["observed_scaffold_hash"] != score["expected_scaffold_hash"]
    assert "unexpected top-level tools" in score["success_reason"]


def test_synthesis_scores_from_final_doc_not_assistant_output() -> None:
    events = [
        _py_repl_start(SCAFFOLD),
        _thread_start("2026-04-05T10:00:01+00:00", "position-for"),
        _thread_start("2026-04-05T10:00:01+00:00", "position-against"),
        _thread_start("2026-04-05T10:00:01+00:00", "critic"),
        _doc_op(
            "2026-04-05T10:00:04+00:00",
            "position-for",
            "write",
            "pro_case_notes",
            "FOR PRO_ANCHOR_SOLAR_17",
        ),
        _doc_op(
            "2026-04-05T10:00:05+00:00",
            "position-against",
            "write",
            "con_case_notes",
            "AGAINST CON_ANCHOR_LEAKAGE_29",
        ),
        _doc_op(
            "2026-04-05T10:00:06+00:00",
            "critic",
            "read",
            "pro_case_notes",
            "FOR PRO_ANCHOR_SOLAR_17",
        ),
        _doc_op(
            "2026-04-05T10:00:07+00:00",
            "critic",
            "read",
            "con_case_notes",
            "AGAINST CON_ANCHOR_LEAKAGE_29",
        ),
        _thread_end("2026-04-05T10:00:10+00:00", "critic"),
        _doc_op(
            "2026-04-05T10:00:11+00:00",
            None,
            "write",
            "final_synthesis",
            "Final doc keeps PRO_ANCHOR_SOLAR_17 and CON_ANCHOR_LEAKAGE_29",
        ),
    ]

    score = score_trace(events, "assistant output dropped anchors", "document-polling", EXPECTATIONS, SCAFFOLD)

    assert score["coordination_success"] is True
    assert score["synthesis_success"] is True
    assert score["final_doc_has_both_markers"] is True
