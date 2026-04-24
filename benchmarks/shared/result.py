"""Result dataclasses for benchmark task and session outcomes."""

from __future__ import annotations

from dataclasses import asdict, dataclass, field
from typing import Any


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

    Field semantics are shared across benchmarks:
    - ``success`` is the benchmark's official pass/fail judgment.
    - ``wall_clock_ms`` is total task-run elapsed time, including harness setup.
    - ``turns`` is the number of Tau ``session/send`` cycles.
    - ``tool_calls`` is the number of tool calls reported by Tau usage events.
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
    metadata: dict[str, Any] = field(default_factory=dict)

    @property
    def total_tokens(self) -> int:
        """Total input + output tokens reported for this task run."""
        return self.input_tokens + self.output_tokens

    def to_dict(self) -> dict[str, Any]:
        """Serialize using the stable report field names."""
        return asdict(self)

    @classmethod
    def from_session(
        cls,
        *,
        task_id: str,
        variant: str,
        run_index: int,
        success: bool,
        wall_clock_ms: int,
        session_result: SessionResult | None,
        turns: int,
        error: str | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> "TaskResult":
        """Build a TaskResult while preserving shared token/turn semantics."""
        return cls(
            task_id=task_id,
            variant=variant,
            run_index=run_index,
            success=success,
            wall_clock_ms=wall_clock_ms,
            input_tokens=session_result.input_tokens if session_result else 0,
            output_tokens=session_result.output_tokens if session_result else 0,
            turns=turns,
            tool_calls=session_result.tool_calls if session_result else 0,
            error=error,
            metadata=metadata or {},
        )
