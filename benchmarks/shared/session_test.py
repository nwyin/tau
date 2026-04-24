from __future__ import annotations

import json
from pathlib import Path

from shared.session import STDERR_TAIL_LINES, TauSession


def rpc_response(req_id: int, result: dict | None = None, error: dict | None = None) -> str:
    msg: dict = {"jsonrpc": "2.0", "id": req_id}
    if error is not None:
        msg["error"] = error
    else:
        msg["result"] = result or {}
    return json.dumps(msg)


def status_notification(status_type: str = "idle", *, output: str = "DONE", tool_calls: int = 1) -> str:
    return json.dumps(
        {
            "jsonrpc": "2.0",
            "method": "session.status",
            "params": {
                "status": {"type": status_type},
                "usage": {
                    "input_tokens": 11,
                    "output_tokens": 7,
                    "tool_calls": tool_calls,
                },
                "output": output,
                "error": "boom" if status_type == "error" else None,
            },
        }
    )


class FakeTauSession(TauSession):
    def __init__(self) -> None:
        super().__init__(model="test-model", cwd=Path("."), timeout=5)
        self.writes: list[dict] = []
        self._lines: list[str] = []
        self.aborted = False

    def _write_line(self, line: str) -> None:
        self.writes.append(json.loads(line))

    def _read_line(self, timeout: float = 1.0) -> str | None:
        if self._lines:
            return self._lines.pop(0)
        return None

    def _best_effort_abort(self) -> None:
        self.aborted = True


def test_send_serializes_system_and_model_overrides() -> None:
    session = FakeTauSession()
    session._lines.extend([rpc_response(1), status_notification(output="DONE", tool_calls=1)])

    result = session.send("run this", system="strict scaffold", model="gpt-5.4-mini")

    assert result.output == "DONE"
    assert result.input_tokens == 11
    assert result.output_tokens == 7
    assert result.tool_calls == 1
    assert session.writes == [
        {
            "jsonrpc": "2.0",
            "method": "session/send",
            "id": 1,
            "params": {
                "prompt": "run this",
                "system": "strict scaffold",
                "model": "gpt-5.4-mini",
            },
        }
    ]


def test_call_buffers_unmatched_notifications_until_send_consumes_them() -> None:
    session = FakeTauSession()
    session._lines.extend([status_notification(output="BUFFERED"), rpc_response(1, {"ok": True})])

    assert session._call("initialize") == {"ok": True}
    assert len(session._pending_messages) == 1

    buffered = session._read_message()
    assert buffered is not None
    assert buffered["method"] == "session.status"
    assert buffered["params"]["output"] == "BUFFERED"


def test_send_consumes_buffered_idle_notification() -> None:
    session = FakeTauSession()
    session._pending_messages.append(json.loads(status_notification(output="FROM BUFFER")))
    session._lines.append(rpc_response(1))

    result = session.send("run this")

    assert result.output == "FROM BUFFER"
    assert result.tool_calls == 1


def test_send_returns_error_result_when_request_is_rejected() -> None:
    session = FakeTauSession()
    session._lines.append(rpc_response(1, error={"code": -32000, "message": "Session is busy"}))

    result = session.send("run this")

    assert result.output == "error: Session is busy"
    assert result.tool_calls == 0
    assert session.turns == 0


def test_send_error_status_preserves_usage() -> None:
    session = FakeTauSession()
    session._lines.extend([rpc_response(1), status_notification("error", tool_calls=3)])

    result = session.send("run this")

    assert result.output == "error: boom"
    assert result.input_tokens == 11
    assert result.output_tokens == 7
    assert result.tool_calls == 3


def test_timeout_result_includes_stderr_tail_and_aborts() -> None:
    session = FakeTauSession()
    session._lines.append(rpc_response(1))
    with session._stderr_lock:
        session._stderr_tail.extend(["first warning", "last warning"])

    result = session.send("run this", timeout=0)

    assert result.output == "error: timeout\nstderr tail:\nfirst warning\nlast warning"
    assert session.aborted


def test_stderr_tail_is_bounded() -> None:
    session = FakeTauSession()
    with session._stderr_lock:
        for index in range(STDERR_TAIL_LINES + 5):
            session._stderr_tail.append(f"line {index}")

    tail = session._stderr_tail_text()
    tail_lines = tail.splitlines()

    assert "line 0" not in tail_lines
    assert "line 4" not in tail_lines
    assert "line 5" in tail_lines
    assert f"line {STDERR_TAIL_LINES + 4}" in tail_lines
