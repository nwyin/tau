from __future__ import annotations

from shared.result import SessionResult, TaskResult


def test_task_result_from_session_preserves_shared_semantics() -> None:
    session = SessionResult(output="ok", input_tokens=3, output_tokens=5, tool_calls=2, wall_clock_ms=17)

    result = TaskResult.from_session(
        task_id="task",
        variant="variant",
        run_index=0,
        success=True,
        wall_clock_ms=25,
        session_result=session,
        turns=1,
        metadata={"score": {"coordination_success": True}},
    )

    assert result.turns == 1
    assert result.tool_calls == 2
    assert result.total_tokens == 8
    assert result.to_dict()["metadata"]["score"]["coordination_success"] is True
