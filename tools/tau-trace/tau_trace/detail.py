"""Detail panel for showing selected item info."""

from __future__ import annotations

import json

from textual.widgets import Static

from .models import ToolCall, ThreadSpan


class DetailPanel(Static):
    """Bottom panel showing details of selected thread or tool call."""

    def on_mount(self) -> None:
        self.update("[dim]Select a thread or tool call to see details[/]")

    def show_thread(self, thread: ThreadSpan) -> None:
        """Show thread details."""
        task_preview = thread.task[:200] if thread.task else ""
        tools_summary = {}
        for tc in thread.tool_calls:
            tools_summary[tc.tool_name] = tools_summary.get(tc.tool_name, 0) + 1
        tools_str = "  ".join(
            f"{name}:{count}" for name, count in sorted(tools_summary.items())
        )

        text = (
            f"[bold]{thread.alias}[/]  {thread.outcome}  "
            f"{thread.duration_ms / 1000:.1f}s  "
            f"model={thread.model}  "
            f"tools={len(thread.tool_calls)}\n"
            f"[dim]Task:[/] {task_preview}\n"
            f"[dim]Tools:[/] {tools_str}"
        )
        self.update(text)

    def show_tool_call(self, tc: ToolCall) -> None:
        """Show tool call details."""
        args_str = json.dumps(tc.args, indent=2)
        if len(args_str) > 500:
            args_str = args_str[:497] + "..."

        result_preview = tc.result_content[:300] if tc.result_content else "(no result)"
        if len(tc.result_content) > 300:
            result_preview += "..."

        err = "[red]ERROR[/] " if tc.is_error else ""
        thread_ctx = f"  thread={tc.thread_alias}" if tc.thread_alias else ""

        text = (
            f"[bold]{tc.tool_name}[/]  {err}{tc.duration_ms}ms{thread_ctx}\n"
            f"[dim]Args:[/] {args_str}\n"
            f"[dim]Result:[/] {result_preview}"
        )
        self.update(text)
