"""Routing graph widget showing episode and document flow."""

from __future__ import annotations

from textual.widgets import Static
from textual.message import Message

from .models import TraceData


class RoutingGraph(Static):
    """ASCII DAG of episode/document flow between threads."""

    class DocSelected(Message):
        """Fired when a document is selected for content preview."""

        def __init__(self, name: str, content: str) -> None:
            self.name = name
            self.content = content
            super().__init__()

    def __init__(self, trace: TraceData) -> None:
        self.trace = trace
        self._doc_names: list[str] = []
        self._selected_doc: int = 0
        super().__init__()

    def on_mount(self) -> None:
        self._rebuild()

    def _outcome_icon(self, outcome: str) -> str:
        if outcome == "completed":
            return "[green]\u2713[/]"
        elif outcome == "timed_out":
            return "[yellow]\u23f1[/]"
        elif outcome == "aborted":
            return "[red]\u2717[/]"
        return "[dim]?[/]"

    def _rebuild(self) -> None:
        t = self.trace
        lines: list[str] = []

        # Header
        lines.append("[bold]Routing Graph[/]")
        lines.append("")

        # Orchestrator node
        lines.append("  [bold cyan]\u25c8 orchestrator[/]")

        # Find which docs the orchestrator wrote
        orch_docs = []
        for d in t.documents:
            if d.thread_alias is None and d.op in ("write", "append"):
                if d.name not in orch_docs:
                    orch_docs.append(d.name)

        for doc_name in orch_docs:
            lines.append(f"  \u2502 [dim]write[/] [yellow]{doc_name}[/]")

        # Thread spawning
        sorted_threads = sorted(t.threads, key=lambda th: th.start_ts)

        # Group parallel threads (same start time within 1s)
        groups: list[list] = []
        for th in sorted_threads:
            if groups and abs(th.start_ts - groups[-1][0].start_ts) < 1.0:
                groups[-1].append(th)
            else:
                groups.append([th])

        for group in groups:
            if len(group) > 1:
                # Parallel threads
                width = max(len(th.alias) for th in group) + 4
                n = len(group)

                # Draw branching lines
                branch_line = "  "
                for i in range(n):
                    if i == 0:
                        branch_line += "\u251c"
                    elif i == n - 1:
                        branch_line += "\u2510"
                    else:
                        branch_line += "\u252c"
                    if i < n - 1:
                        branch_line += "\u2500" * (width - 1)
                lines.append(branch_line)

                # Thread nodes
                node_line = "  "
                for i, th in enumerate(group):
                    icon = self._outcome_icon(th.outcome)
                    alias = th.alias[: width - 2]
                    node_line += f"{icon} [bold]{alias}[/]"
                    if i < n - 1:
                        node_line += " " * (width - len(alias) - 1)
                lines.append(node_line)

                # What docs each thread wrote
                for th in group:
                    for d in t.documents:
                        if d.thread_alias == th.alias and d.op in ("write", "append"):
                            lines.append(
                                f"  {'':>{width * group.index(th) + 2}}\u2502 [dim]write[/] [yellow]{d.name}[/]"
                            )

                # Converging lines for episode injection to next group
                ep_targets = set()
                for ep in t.episodes:
                    for src in ep.source_aliases:
                        if any(th.alias == src for th in group):
                            ep_targets.add(ep.target_alias)

                if ep_targets:
                    merge_line = "  "
                    for i in range(n):
                        if i == 0:
                            merge_line += "\u2514"
                        elif i == n - 1:
                            merge_line += "\u2518"
                        else:
                            merge_line += "\u2534"
                        if i < n - 1:
                            merge_line += "\u2500" * (width - 1)
                    lines.append(merge_line)

                    for target in ep_targets:
                        lines.append(
                            f"  \u2502 [dim]episodes \u2192[/] [green]{target}[/]"
                        )

            else:
                # Sequential thread
                th = group[0]
                icon = self._outcome_icon(th.outcome)
                lines.append("  \u2502")
                lines.append(
                    f"  {icon} [bold]{th.alias}[/]  [dim]{th.duration_ms / 1000:.1f}s[/]"
                )

                # Episode sources
                for ep in t.episodes:
                    if ep.target_alias == th.alias:
                        sources = ", ".join(ep.source_aliases)
                        lines.append(
                            f"    [dim]\u2190 episodes from[/] [cyan]{sources}[/]"
                        )

                # Docs this thread wrote
                for d in t.documents:
                    if d.thread_alias == th.alias and d.op in ("write", "append"):
                        if d.name not in [
                            dd.name
                            for dd in t.documents
                            if dd.thread_alias == th.alias
                            and dd.op in ("write", "append")
                            and t.documents.index(dd) < t.documents.index(d)
                        ]:
                            lines.append(f"    [dim]write[/] [yellow]{d.name}[/]")

        lines.append("")
        lines.append("[bold]Documents[/]")
        lines.append("")

        self._doc_names = t.unique_documents
        for i, name in enumerate(self._doc_names):
            content = t.doc_content(name)
            writers = t.doc_writers(name)
            readers = t.doc_readers(name)
            w_str = ", ".join(writers) if writers else "orchestrator"
            r_str = ", ".join(readers) if readers else "none"
            size = len(content)

            sel = "\u25b6 " if i == self._selected_doc else "  "
            lines.append(f"  {sel}[yellow]{name}[/]  ({size} chars)")
            lines.append(f"      [dim]written by:[/] {w_str}  [dim]read by:[/] {r_str}")

            # Show preview of selected doc
            if i == self._selected_doc and content:
                lines.append(
                    "      [dim]\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500\u2500[/]"
                )
                preview_lines = content.split("\n")[:20]
                for pl in preview_lines:
                    lines.append(f"      [dim]{pl}[/]")
                if len(content.split("\n")) > 20:
                    lines.append(
                        f"      [dim]... ({len(content.split(chr(10)))} total lines)[/]"
                    )
                lines.append("")

        self.update("\n".join(lines))

    def key_up(self) -> None:
        if self._selected_doc > 0:
            self._selected_doc -= 1
            self._rebuild()

    def key_down(self) -> None:
        if self._selected_doc < len(self._doc_names) - 1:
            self._selected_doc += 1
            self._rebuild()
