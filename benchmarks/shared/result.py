"""Result dataclasses for benchmark task and session outcomes."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class SessionResult:
    """Result from a single TauSession.send() call.

    Captures the raw output, token usage, tool call count, and wall-clock
    time for one prompt-response cycle.
    """

    output: str
    input_tokens: int
    output_tokens: int
    tool_calls: int
    wall_clock_ms: int


@dataclass
class TaskResult:
    """Result from running a single benchmark task.

    The ``variant`` field identifies which A/B configuration produced this
    result.  The ``metadata`` dict holds benchmark-specific fields (e.g.
    ``edit_success_rate``, ``recall_accuracy``, ``cycle_count``).
    """

    task_id: str
    variant: str
    run_index: int
    success: bool
    wall_clock_ms: int
    input_tokens: int
    output_tokens: int
    turns: int
    tool_calls: int
    error: str | None = None
    metadata: dict = field(default_factory=dict)
