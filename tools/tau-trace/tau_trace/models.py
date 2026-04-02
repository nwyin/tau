"""Parsed trace data structures."""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class ToolCall:
    tool_call_id: str
    tool_name: str
    args: dict
    thread_alias: str | None
    thread_id: str | None
    start_ts: float  # seconds from session start
    duration_ms: int = 0
    is_error: bool = False
    result_content: str = ""


@dataclass
class ThreadSpan:
    alias: str
    thread_id: str
    task: str
    model: str
    outcome: str  # "completed", "aborted", "escalated", "timed_out"
    start_ts: float  # seconds from session start
    end_ts: float
    duration_ms: int
    tool_calls: list[ToolCall] = field(default_factory=list)


@dataclass
class EpisodeInject:
    source_aliases: list[str]
    target_alias: str
    target_thread_id: str
    ts: float


@dataclass
class DocumentOp:
    op: str  # read, write, append, list
    name: str
    content: str
    thread_alias: str | None
    ts: float


@dataclass
class EvidenceCite:
    thread_alias: str
    thread_id: str
    tool_call_ids: list[str]
    ts: float


@dataclass
class TurnInfo:
    input_tokens: int
    output_tokens: int
    ts: float


@dataclass
class TraceData:
    session_duration_s: float = 0.0
    model: str = ""
    threads: list[ThreadSpan] = field(default_factory=list)
    tool_calls: list[ToolCall] = field(default_factory=list)
    episodes: list[EpisodeInject] = field(default_factory=list)
    documents: list[DocumentOp] = field(default_factory=list)
    evidence: list[EvidenceCite] = field(default_factory=list)
    turns: list[TurnInfo] = field(default_factory=list)
    total_input_tokens: int = 0
    total_output_tokens: int = 0
    start_ts_iso: str = ""

    @property
    def total_tool_calls(self) -> int:
        return len(self.tool_calls)

    @property
    def thread_count(self) -> int:
        return len(self.threads)

    @property
    def completed_threads(self) -> int:
        return sum(1 for t in self.threads if t.outcome == "completed")

    @property
    def unique_documents(self) -> list[str]:
        seen = set()
        names = []
        for d in self.documents:
            if d.name and d.name not in seen:
                seen.add(d.name)
                names.append(d.name)
        return names

    def doc_content(self, name: str) -> str:
        """Get the latest written content for a document."""
        content = ""
        for d in self.documents:
            if d.name == name and d.op in ("write", "append"):
                if d.op == "write":
                    content = d.content
                else:
                    content += d.content
        return content

    def doc_readers(self, name: str) -> list[str]:
        readers = set()
        for d in self.documents:
            if d.name == name and d.op == "read" and d.thread_alias:
                readers.add(d.thread_alias)
        return sorted(readers)

    def doc_writers(self, name: str) -> list[str]:
        writers = set()
        for d in self.documents:
            if d.name == name and d.op in ("write", "append") and d.thread_alias:
                writers.add(d.thread_alias)
        return sorted(writers)


def _parse_ts(iso: str, base: float) -> float:
    """Convert ISO timestamp to seconds from base timestamp."""
    from datetime import datetime, timezone

    try:
        dt = datetime.fromisoformat(iso)
        if dt.tzinfo is None:
            dt = dt.replace(tzinfo=timezone.utc)
        return dt.timestamp() - base
    except (ValueError, TypeError):
        return 0.0


def load_trace(path: Path) -> TraceData:
    """Load and parse a trace.jsonl file into structured data."""
    events = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if line:
                try:
                    events.append(json.loads(line))
                except json.JSONDecodeError:
                    continue

    if not events:
        return TraceData()

    # Find base timestamp
    base_ts = 0.0
    for ev in events:
        if ev.get("event") == "agent_start" and "ts" in ev:
            from datetime import datetime, timezone

            dt = datetime.fromisoformat(ev["ts"])
            if dt.tzinfo is None:
                dt = dt.replace(tzinfo=timezone.utc)
            base_ts = dt.timestamp()
            break

    if base_ts == 0.0 and events and "ts" in events[0]:
        from datetime import datetime, timezone

        dt = datetime.fromisoformat(events[0]["ts"])
        if dt.tzinfo is None:
            dt = dt.replace(tzinfo=timezone.utc)
        base_ts = dt.timestamp()

    trace = TraceData()
    trace.start_ts_iso = events[0].get("ts", "")

    # Track thread starts for building spans
    thread_starts: dict[str, dict] = {}
    # Track tool starts for pairing with ends
    tool_starts: dict[str, dict] = {}

    for ev in events:
        event_type = ev.get("event", "")
        ts = _parse_ts(ev.get("ts", ""), base_ts)

        if event_type == "agent_start":
            pass

        elif event_type == "agent_end":
            trace.session_duration_s = ts

        elif event_type == "thread_start":
            alias = ev.get("alias", "")
            thread_starts[alias] = ev
            if not trace.model and ev.get("model"):
                trace.model = ev["model"]

        elif event_type == "thread_end":
            alias = ev.get("alias", "")
            start_ev = thread_starts.pop(alias, None)
            start_ts = _parse_ts(start_ev["ts"], base_ts) if start_ev else 0.0
            span = ThreadSpan(
                alias=alias,
                thread_id=ev.get("thread_id", ""),
                task=start_ev.get("task", "") if start_ev else "",
                model=start_ev.get("model", "") if start_ev else "",
                outcome=ev.get("outcome", "unknown"),
                start_ts=start_ts,
                end_ts=ts,
                duration_ms=ev.get("duration_ms", 0),
            )
            trace.threads.append(span)

        elif event_type == "tool_start":
            tool_starts[ev.get("tool_call_id", "")] = ev

        elif event_type == "tool_end":
            tc_id = ev.get("tool_call_id", "")
            start_ev = tool_starts.pop(tc_id, None)
            start_ts = _parse_ts(start_ev["ts"], base_ts) if start_ev else ts
            tc = ToolCall(
                tool_call_id=tc_id,
                tool_name=ev.get("tool_name", ""),
                args=start_ev.get("args", {}) if start_ev else {},
                thread_alias=ev.get("thread_alias"),
                thread_id=ev.get("thread_id"),
                start_ts=start_ts,
                duration_ms=ev.get("duration_ms", 0),
                is_error=ev.get("is_error", False),
                result_content=ev.get("result_content", ""),
            )
            trace.tool_calls.append(tc)

        elif event_type == "episode_inject":
            trace.episodes.append(
                EpisodeInject(
                    source_aliases=ev.get("source_aliases", []),
                    target_alias=ev.get("target_alias", ""),
                    target_thread_id=ev.get("target_thread_id", ""),
                    ts=ts,
                )
            )

        elif event_type == "document_op":
            trace.documents.append(
                DocumentOp(
                    op=ev.get("op", ""),
                    name=ev.get("name", ""),
                    content=ev.get("content", ""),
                    thread_alias=ev.get("thread_alias"),
                    ts=ts,
                )
            )

        elif event_type == "evidence_cite":
            trace.evidence.append(
                EvidenceCite(
                    thread_alias=ev.get("thread_alias", ""),
                    thread_id=ev.get("thread_id", ""),
                    tool_call_ids=ev.get("tool_call_ids", []),
                    ts=ts,
                )
            )

        elif event_type == "turn_end":
            ti = TurnInfo(
                input_tokens=ev.get("input_tokens", 0),
                output_tokens=ev.get("output_tokens", 0),
                ts=ts,
            )
            trace.turns.append(ti)
            trace.total_input_tokens += ti.input_tokens
            trace.total_output_tokens += ti.output_tokens

    # Assign tool calls to threads (post-parse, since threads may be created after their tool events)
    thread_map = {t.alias: t for t in trace.threads}
    for tc in trace.tool_calls:
        if tc.thread_alias and tc.thread_alias in thread_map:
            thread_map[tc.thread_alias].tool_calls.append(tc)

    return trace
