"""Gantt chart widget showing thread timeline with expandable tool calls."""

from __future__ import annotations


from textual.widgets import Static
from textual.reactive import reactive
from textual.message import Message

from .models import TraceData, ThreadSpan, ToolCall


# Colors for threads (cycle through these)
THREAD_COLORS = ["cyan", "green", "magenta", "yellow", "blue", "red"]


def _outcome_icon(outcome: str) -> str:
    if outcome == "completed":
        return "[green]\u2713[/]"
    elif outcome == "timed_out":
        return "[yellow]\u23f1[/]"
    elif outcome == "aborted":
        return "[red]\u2717[/]"
    elif outcome == "escalated":
        return "[yellow]\u26a0[/]"
    return "[dim]?[/]"


def _bar(start_ts: float, end_ts: float, total_s: float, width: int, color: str) -> str:
    """Render a horizontal bar within a fixed-width field."""
    if total_s <= 0:
        return " " * width

    col_start = int(start_ts / total_s * width)
    col_end = int(end_ts / total_s * width)
    col_start = max(0, min(col_start, width - 1))
    col_end = max(col_start + 1, min(col_end, width))

    before = " " * col_start
    bar = "\u2588" * (col_end - col_start)
    after = " " * (width - col_end)
    return f"{before}[{color}]{bar}[/]{after}"


def _tool_tick(tc: ToolCall, total_s: float, width: int) -> str:
    """Render a single tool call as a tick mark."""
    if total_s <= 0:
        return " " * width

    col = int(tc.start_ts / total_s * width)
    col = max(0, min(col, width - 1))

    chars = [" "] * width
    chars[col] = "\u2502"
    return "".join(chars)


def _format_arg_preview(tc: ToolCall) -> str:
    """Extract a short arg preview for a tool call."""
    args = tc.args
    for key in (
        "path",
        "command",
        "pattern",
        "name",
        "operation",
        "prompt",
        "alias",
        "task",
    ):
        if key in args:
            val = str(args[key])
            if len(val) > 40:
                val = val[:37] + "..."
            return val
    return ""


class GanttChart(Static):
    """Thread timeline with expandable tool call rows."""

    selected_idx: reactive[int] = reactive(0)
    expanded: reactive[set] = reactive(set)

    class ToolSelected(Message):
        """Fired when a tool call is selected for detail view."""

        def __init__(self, tool_call: ToolCall) -> None:
            self.tool_call = tool_call
            super().__init__()

    class ThreadSelected(Message):
        """Fired when a thread is selected."""

        def __init__(self, thread: ThreadSpan) -> None:
            self.thread = thread
            super().__init__()

    def __init__(self, trace: TraceData) -> None:
        self.trace = trace
        self._expanded: set[str] = set()
        self._row_map: list[
            tuple[str, str | None]
        ] = []  # (thread_alias, tool_call_id | None)
        super().__init__()

    def on_mount(self) -> None:
        self._rebuild()

    def _rebuild(self) -> None:
        """Rebuild the display."""
        t = self.trace
        total_s = t.session_duration_s or 1.0

        # Available width for the bar area
        size = self.size
        label_w = 12
        bar_w = max(30, size.width - label_w - 16)  # leave room for duration + icon

        lines: list[str] = []
        self._row_map = []

        # Time axis
        steps = 5
        axis_chars = [" "] * bar_w
        for i in range(steps + 1):
            sec = int(total_s * i / steps)
            pos = min(int(bar_w * i / steps), bar_w - 1)
            label = f"{sec}s"
            for j, ch in enumerate(label):
                if pos + j < bar_w:
                    axis_chars[pos + j] = ch
        axis_str = "".join(axis_chars)
        lines.append(f"[dim]{' ' * label_w}{axis_str}[/]")
        lines.append(f"[dim]{' ' * label_w}{'\u2500' * bar_w}[/]")

        # Thread rows
        sorted_threads = sorted(t.threads, key=lambda th: th.start_ts)
        for i, th in enumerate(sorted_threads):
            color = THREAD_COLORS[i % len(THREAD_COLORS)]
            icon = _outcome_icon(th.outcome)
            bar = _bar(th.start_ts, th.end_ts, total_s, bar_w, color)
            duration_str = f"{th.duration_ms / 1000:.1f}s"

            # Selection indicator
            is_selected = len(lines) - 2 == self.selected_idx  # offset by axis lines
            prefix = "\u25ba" if th.alias in self._expanded else "\u25b6"
            sel_mark = "[reverse]" if is_selected else ""
            sel_end = "[/]" if is_selected else ""

            alias_padded = th.alias[: label_w - 2].ljust(label_w - 2)
            line = (
                f"{sel_mark}{prefix} {alias_padded}{sel_end}{bar} {icon} {duration_str}"
            )
            lines.append(line)
            self._row_map.append((th.alias, None))

            # Expanded tool calls
            if th.alias in self._expanded:
                for tc in th.tool_calls:
                    arg_preview = _format_arg_preview(tc)
                    tool_label = f"{tc.tool_name}"
                    if arg_preview:
                        tool_label += f" {arg_preview}"
                    tool_label = tool_label[: label_w + 15]

                    tick = _tool_tick(tc, total_s, bar_w)
                    dur = f"{tc.duration_ms}ms" if tc.duration_ms else ""
                    err = "[red]ERR[/] " if tc.is_error else ""

                    indent = "  \u251c " if tc != th.tool_calls[-1] else "  \u2514 "
                    tc_line = (
                        f"[dim]{indent}{tool_label:<{label_w + 12}}[/]{tick} {err}{dur}"
                    )
                    lines.append(tc_line)
                    self._row_map.append((th.alias, tc.tool_call_id))

        # Separator
        lines.append("")

        # Episode injections
        if t.episodes:
            lines.append("[bold]Episodes[/]")
            for ep in t.episodes:
                sources = ", ".join(ep.source_aliases)
                lines.append(
                    f"  [cyan]{sources}[/] \u2500\u2500\u25b6 [green]{ep.target_alias}[/]"
                )

        # Documents
        if t.unique_documents:
            lines.append("[bold]Documents[/]")
            for name in t.unique_documents:
                writers = t.doc_writers(name)
                readers = t.doc_readers(name)
                w_str = ", ".join(writers) if writers else "orch"
                r_str = ", ".join(readers) if readers else ""
                content = t.doc_content(name)
                size_str = f"{len(content)} chars"
                line = f"  [yellow]{name}[/] ({size_str})  [dim]W:{w_str}[/]"
                if r_str:
                    line += f"  [dim]R:{r_str}[/]"
                lines.append(line)

        self.update("\n".join(lines))

    def key_up(self) -> None:
        if self.selected_idx > 0:
            self.selected_idx -= 1
            self._rebuild()

    def key_down(self) -> None:
        if self.selected_idx < len(self._row_map) - 1:
            self.selected_idx += 1
            self._rebuild()

    def key_enter(self) -> None:
        if not self._row_map or self.selected_idx >= len(self._row_map):
            return
        alias, tc_id = self._row_map[self.selected_idx]

        if tc_id is not None:
            # Tool call selected — show detail
            for tc in self.trace.tool_calls:
                if tc.tool_call_id == tc_id:
                    self.post_message(self.ToolSelected(tc))
                    break
        else:
            # Thread row — toggle expand
            if alias in self._expanded:
                self._expanded.discard(alias)
            else:
                self._expanded.add(alias)
            self._rebuild()

    def watch_selected_idx(self, value: int) -> None:
        if self._row_map and value < len(self._row_map):
            alias, tc_id = self._row_map[value]
            if tc_id is None:
                for th in self.trace.threads:
                    if th.alias == alias:
                        self.post_message(self.ThreadSelected(th))
                        break
