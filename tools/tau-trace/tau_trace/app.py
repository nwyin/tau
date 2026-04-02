"""Textual app for trace visualization."""

from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.widgets import Footer, Static, TabbedContent, TabPane

from .models import TraceData
from .gantt import GanttChart
from .routing import RoutingGraph
from .detail import DetailPanel


class SummaryHeader(Static):
    """Always-visible summary bar at top."""

    def __init__(self, trace: TraceData) -> None:
        self.trace = trace
        super().__init__()

    def compose(self) -> ComposeResult:
        t = self.trace

        # Thread outcome icons
        outcomes = []
        for th in t.threads:
            if th.outcome == "completed":
                outcomes.append(f"[green]\u2713[/] {th.alias}")
            elif th.outcome == "timed_out":
                outcomes.append(f"[yellow]\u23f1[/] {th.alias}")
            else:
                outcomes.append(f"[red]\u2717[/] {th.alias}")
        thread_str = "  ".join(outcomes) if outcomes else "none"

        duration = f"{t.session_duration_s:.0f}s" if t.session_duration_s else "?"
        tokens = f"{t.total_input_tokens:,} in / {t.total_output_tokens:,} out"
        docs = ", ".join(t.unique_documents) if t.unique_documents else "none"

        text = (
            f"[bold]Session[/]  {t.start_ts_iso[:19]}  |  "
            f"[bold]{duration}[/]  |  "
            f"{t.model}\n"
            f"[bold]Threads[/]  {thread_str}  |  "
            f"[bold]Tools[/] {t.total_tool_calls}  |  "
            f"[bold]Episodes[/] {len(t.episodes)}  |  "
            f"[bold]Evidence[/] {len(t.evidence)}  |  "
            f"[bold]Tokens[/] {tokens}\n"
            f"[bold]Docs[/]  {docs}"
        )

        yield Static(text, id="summary-text")


class TraceApp(App):
    """TUI trace viewer for tau agent orchestration."""

    CSS = """
    SummaryHeader {
        height: 4;
        padding: 0 1;
        background: $surface;
        border-bottom: solid $primary;
    }

    #summary-text {
        height: 3;
    }

    TabbedContent {
        height: 1fr;
    }

    GanttChart {
        height: 1fr;
        padding: 0 1;
    }

    RoutingGraph {
        height: 1fr;
        padding: 0 1;
    }

    DetailPanel {
        height: auto;
        max-height: 12;
        padding: 0 1;
        border-top: solid $primary;
        background: $surface;
    }

    .thread-row {
        height: 1;
    }

    .thread-row-selected {
        height: 1;
        background: $accent 20%;
    }

    .tool-row {
        height: 1;
        color: $text-muted;
    }
    """

    BINDINGS = [
        Binding("q", "quit", "Quit"),
        Binding("tab", "next_tab", "Next Tab", show=False),
    ]

    def __init__(self, trace: TraceData) -> None:
        self.trace = trace
        super().__init__()

    def compose(self) -> ComposeResult:
        yield SummaryHeader(self.trace)
        with TabbedContent():
            with TabPane("Timeline", id="timeline-tab"):
                yield GanttChart(self.trace)
            with TabPane("Routing", id="routing-tab"):
                yield RoutingGraph(self.trace)
        yield DetailPanel()
        yield Footer()

    def on_gantt_chart_thread_selected(self, event: GanttChart.ThreadSelected) -> None:
        self.query_one(DetailPanel).show_thread(event.thread)

    def on_gantt_chart_tool_selected(self, event: GanttChart.ToolSelected) -> None:
        self.query_one(DetailPanel).show_tool_call(event.tool_call)

    def action_next_tab(self) -> None:
        tabs = self.query_one(TabbedContent)
        tabs.action_next_tab()
