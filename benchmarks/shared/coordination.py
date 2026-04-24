"""Shared fixture loading and trace analysis for coordination benchmarks."""

from __future__ import annotations

import hashlib
import json
import textwrap
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any


@dataclass
class CoordinationExpectations:
    """Task-specific expectations used by coordination benchmark scorers."""

    pro_alias: str
    con_alias: str
    critic_alias: str
    pro_doc: str
    con_doc: str
    pro_markers: list[str]
    con_markers: list[str]
    pro_task: str
    con_task: str
    critic_task: str
    final_doc: str = "final_synthesis"
    synthesis_task: str | None = None

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
            pro_task=metadata.get("pro_task", "").strip(),
            con_task=metadata.get("con_task", "").strip(),
            critic_task=metadata.get("critic_task", "").strip(),
            final_doc=metadata.get("final_doc", "final_synthesis"),
            synthesis_task=metadata.get("synthesis_task"),
        )


def load_coordination_tasks(fixtures_dir: Path) -> list[dict[str, Any]]:
    """Load coordination benchmark tasks from fixture directories."""
    tasks: list[dict[str, Any]] = []
    for task_dir in sorted(fixtures_dir.iterdir()):
        if not task_dir.is_dir():
            continue

        prompt_path = task_dir / "prompt.md"
        metadata_path = task_dir / "metadata.json"
        if not prompt_path.exists() or not metadata_path.exists():
            continue

        metadata = json.loads(metadata_path.read_text())
        tasks.append(
            {
                "id": task_dir.name,
                "dir": task_dir,
                "prompt": prompt_path.read_text().strip(),
                "metadata": metadata,
                "expectations": CoordinationExpectations.from_metadata(metadata),
            }
        )
    return tasks


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
            continue
    return events


def normalize_scaffold(code: str) -> str:
    """Normalize scaffold text for hash-stable fidelity checks."""
    normalized = textwrap.dedent(code).replace("\r\n", "\n").replace("\r", "\n")
    lines = [line.rstrip() for line in normalized.strip().splitlines()]
    if not lines:
        return ""
    return "\n".join(lines) + "\n"


def scaffold_hash(code: str) -> str:
    """Return a stable hash for a normalized scaffold."""
    return hashlib.sha256(normalize_scaffold(code).encode("utf-8")).hexdigest()


def collect_trace_state(
    events: list[dict[str, Any]],
    expectations: CoordinationExpectations,
) -> dict[str, Any]:
    """Collect coordination-relevant facts from a trace."""

    thread_start: dict[str, datetime] = {}
    thread_end: dict[str, datetime] = {}
    doc_writes: dict[str, list[datetime]] = {}
    critic_doc_reads: dict[str, list[datetime]] = {}
    document_contents: dict[str, str] = {}
    top_level_tool_starts: list[dict[str, Any]] = []

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

        if event_type == "tool_start":
            if event.get("thread_alias") is None:
                top_level_tool_starts.append(event)

            if event.get("tool_name") == "document" and ts is not None:
                thread_alias = event.get("thread_alias")
                args = event.get("args", {})
                if not isinstance(args, dict):
                    continue

                operation = args.get("operation")
                name = args.get("name")
                content = args.get("content", "")
                if not isinstance(operation, str) or not isinstance(name, str):
                    continue

                _record_document_event(
                    thread_alias=thread_alias,
                    operation=operation,
                    name=name,
                    content=content if isinstance(content, str) else "",
                    ts=ts,
                    expectations=expectations,
                    doc_writes=doc_writes,
                    critic_doc_reads=critic_doc_reads,
                    document_contents=document_contents,
                )

        if event_type == "document_op" and ts is not None:
            thread_alias = event.get("thread_alias")
            operation = event.get("op")
            name = event.get("name")
            content = event.get("content", "")
            if not isinstance(operation, str) or not isinstance(name, str):
                continue

            _record_document_event(
                thread_alias=thread_alias,
                operation=operation,
                name=name,
                content=content if isinstance(content, str) else "",
                ts=ts,
                expectations=expectations,
                doc_writes=doc_writes,
                critic_doc_reads=critic_doc_reads,
                document_contents=document_contents,
            )

    return {
        "thread_start": thread_start,
        "thread_end": thread_end,
        "doc_writes": doc_writes,
        "critic_doc_reads": critic_doc_reads,
        "document_contents": document_contents,
        "top_level_tool_starts": top_level_tool_starts,
        "episode_inject_count_to_critic": episode_inject_count,
        "episode_inject_has_both_sources": episode_inject_has_both_sources,
        "citations_by_critic": citations_by_critic,
    }


def score_coordination_from_state(
    state: dict[str, Any],
    content_text: str,
    variant_name: str,
    expectations: CoordinationExpectations,
) -> dict[str, Any]:
    """Score common coordination mechanics from extracted trace state."""

    doc_writes: dict[str, list[datetime]] = state["doc_writes"]
    critic_doc_reads: dict[str, list[datetime]] = state["critic_doc_reads"]
    thread_start: dict[str, datetime] = state["thread_start"]
    thread_end: dict[str, datetime] = state["thread_end"]

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

    required_docs = [expectations.pro_doc, expectations.con_doc]
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

    content_lc = content_text.lower()
    has_pro_marker = any(marker.lower() in content_lc for marker in expectations.pro_markers)
    has_con_marker = any(marker.lower() in content_lc for marker in expectations.con_markers)
    content_has_both_markers = has_pro_marker and has_con_marker

    episode_mechanism_ok = bool(state["episode_inject_has_both_sources"])
    document_mechanism_ok = read_both_docs_after_write

    expected_mechanism = expected_coordination_mechanism(variant_name)
    mechanism_success = (
        episode_mechanism_ok
        if expected_mechanism == "episode"
        else document_mechanism_ok
        if expected_mechanism == "document"
        else (episode_mechanism_ok or document_mechanism_ok)
    )
    observed_coordination = episode_mechanism_ok or document_mechanism_ok
    timing_success = bool(writes_available and critic_ended_after_required_writes is True)
    synthesis_success = content_has_both_markers

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
    elif writes_available and not timing_success:
        fail_reasons.append("critic timing could not be confirmed after required artifacts were available")

    coordination_success = writes_available and mechanism_success and timing_success
    if coordination_success and fail_reasons:
        fail_reasons = []
    success_reason = "ok" if coordination_success else "; ".join(fail_reasons)

    return {
        "coordination_success": coordination_success,
        "mechanism_success": mechanism_success,
        "timing_success": timing_success,
        "synthesis_success": synthesis_success,
        "success_reason": success_reason,
        "required_docs_written": writes_available,
        "expected_mechanism": expected_mechanism,
        "episode_inject_count_to_critic": state["episode_inject_count_to_critic"],
        "episode_inject_has_both_sources": state["episode_inject_has_both_sources"],
        "critic_doc_reads_total": critic_doc_reads_total,
        "critic_doc_reads_required": len(docs_read_by_critic),
        "critic_doc_reads_after_required_writes": len(docs_read_after_write),
        "critic_docs_read": sorted(docs_read_by_critic),
        "critic_docs_read_after_write": sorted(docs_read_after_write),
        "citations_by_critic": state["citations_by_critic"],
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


def expected_coordination_mechanism(variant_name: str) -> str:
    """Return the expected coordination mechanism for a variant."""
    if variant_name == "staged-pipeline":
        return "episode"
    if variant_name == "document-polling":
        return "document"
    return "either"


def document_content(events: list[dict[str, Any]], name: str) -> str:
    """Reconstruct the latest content for a document from trace events."""
    content = ""
    for event in events:
        event_type = event.get("event")
        if event_type != "document_op":
            continue
        if event.get("name") != name:
            continue

        op = event.get("op")
        payload = event.get("content", "")
        if not isinstance(payload, str):
            payload = ""

        if op == "write":
            content = payload
        elif op == "append":
            content += payload
    return content


def _record_document_event(
    *,
    thread_alias: Any,
    operation: str,
    name: str,
    content: str,
    ts: datetime,
    expectations: CoordinationExpectations,
    doc_writes: dict[str, list[datetime]],
    critic_doc_reads: dict[str, list[datetime]],
    document_contents: dict[str, str],
) -> None:
    if thread_alias in (expectations.pro_alias, expectations.con_alias) and operation in ("write", "append"):
        doc_writes.setdefault(name, []).append(ts)
    if thread_alias == expectations.critic_alias and operation == "read":
        critic_doc_reads.setdefault(name, []).append(ts)

    if operation == "write":
        document_contents[name] = content
    elif operation == "append":
        document_contents[name] = document_contents.get(name, "") + content


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
