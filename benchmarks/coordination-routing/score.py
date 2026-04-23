"""Trace-first scoring for the coordination-routing benchmark."""

from __future__ import annotations

import json
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any


@dataclass
class CoordinationExpectations:
    """Task-specific expectations used by the coordination scorer."""

    pro_alias: str
    con_alias: str
    critic_alias: str
    pro_doc: str
    con_doc: str
    pro_markers: list[str]
    con_markers: list[str]

    @classmethod
    def from_metadata(cls, metadata: dict[str, Any]) -> "CoordinationExpectations":
        return cls(
            pro_alias=metadata["pro_alias"],
            con_alias=metadata["con_alias"],
            critic_alias=metadata["critic_alias"],
            pro_doc=metadata["pro_doc"],
            con_doc=metadata["con_doc"],
            pro_markers=list(metadata["pro_markers"]),
            con_markers=list(metadata["con_markers"]),
        )


def load_trace_events(trace_path: Path) -> list[dict[str, Any]]:
    """Load trace events from a JSONL file."""
    events: list[dict[str, Any]] = []
    if not trace_path.exists():
        return events

    for line in trace_path.read_text().splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            events.append(json.loads(line))
        except json.JSONDecodeError:
            # Skip malformed lines; benchmark should stay robust to partial traces.
            continue
    return events


def score_trace(
    events: list[dict[str, Any]],
    output_text: str,
    variant_name: str,
    expectations: CoordinationExpectations,
) -> dict[str, Any]:
    """Score a run using trace evidence and final output anchors."""

    thread_start: dict[str, datetime] = {}
    thread_end: dict[str, datetime] = {}
    doc_writes: dict[str, list[datetime]] = {}
    critic_doc_reads: dict[str, list[datetime]] = {}

    episode_inject_count = 0
    episode_inject_has_both_sources = False
    citations_by_critic = 0

    for event in events:
        ts = _parse_ts(event.get("ts"))
        event_type = event.get("event")
        if not event_type:
            continue

        if event_type == "thread_start" and ts is not None:
            alias = event.get("alias")
            if isinstance(alias, str) and alias not in thread_start:
                thread_start[alias] = ts

        if event_type == "thread_end" and ts is not None:
            alias = event.get("alias")
            if isinstance(alias, str) and alias not in thread_end:
                thread_end[alias] = ts

        if event_type == "episode_inject":
            target_alias = event.get("target_alias")
            if target_alias == expectations.critic_alias:
                episode_inject_count += 1
                sources = set(event.get("source_aliases", []))
                needed = {expectations.pro_alias, expectations.con_alias}
                if needed.issubset(sources):
                    episode_inject_has_both_sources = True

        if event_type == "evidence_cite" and event.get("thread_alias") == expectations.critic_alias:
            cited = event.get("tool_call_ids")
            if isinstance(cited, list) and cited:
                citations_by_critic += len(cited)
            else:
                citations_by_critic += 1

        # Primary signal: document tool calls tagged with thread alias.
        if event_type == "tool_start" and event.get("tool_name") == "document" and ts is not None:
            thread_alias = event.get("thread_alias")
            args = event.get("args", {})
            if not isinstance(args, dict):
                continue

            operation = args.get("operation")
            name = args.get("name")
            if not isinstance(operation, str) or not isinstance(name, str):
                continue

            if thread_alias in (expectations.pro_alias, expectations.con_alias) and operation in ("write", "append"):
                doc_writes.setdefault(name, []).append(ts)

            if thread_alias == expectations.critic_alias and operation == "read":
                critic_doc_reads.setdefault(name, []).append(ts)

        # Secondary signal if traces include thread_alias for document_op.
        if event_type == "document_op" and ts is not None:
            thread_alias = event.get("thread_alias")
            operation = event.get("op")
            name = event.get("name")
            if not isinstance(operation, str) or not isinstance(name, str):
                continue

            if thread_alias in (expectations.pro_alias, expectations.con_alias) and operation in ("write", "append"):
                doc_writes.setdefault(name, []).append(ts)

            if thread_alias == expectations.critic_alias and operation == "read":
                critic_doc_reads.setdefault(name, []).append(ts)

    required_docs = [expectations.pro_doc, expectations.con_doc]
    pro_write = _first_ts(doc_writes.get(expectations.pro_doc, []))
    con_write = _first_ts(doc_writes.get(expectations.con_doc, []))
    writes_available = pro_write is not None and con_write is not None
    latest_required_write = _latest_ts([pro_write, con_write])

    critic_start = thread_start.get(expectations.critic_alias)
    critic_end = thread_end.get(expectations.critic_alias)

    critic_started_after_required_writes = (
        critic_start >= latest_required_write
        if critic_start is not None and latest_required_write is not None
        else None
    )
    critic_ended_after_required_writes = (
        critic_end >= latest_required_write
        if critic_end is not None and latest_required_write is not None
        else None
    )
    critic_finished_before_required_writes = (
        critic_end < latest_required_write
        if critic_end is not None and latest_required_write is not None
        else None
    )

    docs_read_by_critic: list[str] = []
    docs_read_after_write: list[str] = []
    for doc_name in required_docs:
        reads = sorted(critic_doc_reads.get(doc_name, []))
        write_ts = _first_ts(doc_writes.get(doc_name, []))
        if reads:
            docs_read_by_critic.append(doc_name)
        if write_ts is not None and any(read_ts >= write_ts for read_ts in reads):
            docs_read_after_write.append(doc_name)

    read_both_docs_after_write = len(docs_read_after_write) == len(required_docs)
    critic_doc_reads_total = sum(len(reads) for reads in critic_doc_reads.values())

    output_lc = output_text.lower()
    has_pro_marker = any(marker.lower() in output_lc for marker in expectations.pro_markers)
    has_con_marker = any(marker.lower() in output_lc for marker in expectations.con_markers)
    content_has_both_markers = has_pro_marker and has_con_marker

    episode_mechanism_ok = episode_inject_has_both_sources
    document_mechanism_ok = read_both_docs_after_write
    observed_coordination = episode_mechanism_ok or document_mechanism_ok

    expected_mechanism = _expected_mechanism(variant_name)

    fail_reasons: list[str] = []
    if not writes_available:
        fail_reasons.append("required docs were not written by upstream threads")
    if expected_mechanism == "episode" and not episode_mechanism_ok:
        fail_reasons.append("critic did not receive episode injection from both upstream aliases")
    if expected_mechanism == "document" and not document_mechanism_ok:
        fail_reasons.append("critic did not read both required docs after they were written")
    if expected_mechanism == "either" and not observed_coordination:
        fail_reasons.append("no coordination mechanism observed (no useful episode inject or doc reads)")
    if critic_ended_after_required_writes is False:
        fail_reasons.append("critic ended before required artifacts were available")
    coordination_success = len(fail_reasons) == 0
    success_reason = "ok" if coordination_success else "; ".join(fail_reasons)

    return {
        "coordination_success": coordination_success,
        "success_reason": success_reason,
        "trace_event_count": len(events),
        "required_docs_written": writes_available,
        "expected_mechanism": expected_mechanism,
        "episode_inject_count_to_critic": episode_inject_count,
        "episode_inject_has_both_sources": episode_inject_has_both_sources,
        "critic_doc_reads_total": critic_doc_reads_total,
        "critic_doc_reads_required": len(docs_read_by_critic),
        "critic_doc_reads_after_required_writes": len(docs_read_after_write),
        "critic_docs_read": sorted(docs_read_by_critic),
        "critic_docs_read_after_write": sorted(docs_read_after_write),
        "citations_by_critic": citations_by_critic,
        "content_has_pro_marker": has_pro_marker,
        "content_has_con_marker": has_con_marker,
        "content_has_both_markers": content_has_both_markers,
        "observed_coordination": observed_coordination,
        "critic_started_after_required_writes": critic_started_after_required_writes,
        "critic_ended_after_required_writes": critic_ended_after_required_writes,
        "critic_finished_before_required_writes": critic_finished_before_required_writes,
        "pro_write_ts": _ts_to_str(pro_write),
        "con_write_ts": _ts_to_str(con_write),
        "critic_start_ts": _ts_to_str(critic_start),
        "critic_end_ts": _ts_to_str(critic_end),
    }


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
            "success_reason": f"trace missing or empty: {trace_path}",
            "trace_event_count": 0,
            "required_docs_written": False,
            "expected_mechanism": _expected_mechanism(variant_name),
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
        }
    return score_trace(events, output_text, variant_name, expectations)


def _expected_mechanism(variant_name: str) -> str:
    if variant_name == "staged-pipeline":
        return "episode"
    if variant_name == "document-polling":
        return "document"
    return "either"


def _parse_ts(value: Any) -> datetime | None:
    if not isinstance(value, str) or not value:
        return None
    try:
        return datetime.fromisoformat(value)
    except ValueError:
        return None


def _first_ts(values: list[datetime]) -> datetime | None:
    if not values:
        return None
    return min(values)


def _latest_ts(values: list[datetime | None]) -> datetime | None:
    non_null = [value for value in values if value is not None]
    if not non_null:
        return None
    return max(non_null)


def _ts_to_str(value: datetime | None) -> str | None:
    return value.isoformat() if value is not None else None
