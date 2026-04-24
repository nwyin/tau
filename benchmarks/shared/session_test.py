from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from shared.session import TauSession


class FakeTauSession(TauSession):
    def __init__(self) -> None:
        super().__init__(model="test-model", cwd=Path("."), timeout=5)
        self.calls: list[tuple[str, dict | None]] = []
        self._lines: list[str] = []

    def _call(self, method: str, params: dict | None = None) -> dict:
        self.calls.append((method, params))
        return {}

    def _read_line(self, timeout: float = 1.0) -> str | None:
        if self._lines:
            return self._lines.pop(0)
        return None


def test_send_serializes_system_and_model_overrides() -> None:
    session = FakeTauSession()
    session._lines.append(
        json.dumps(
            {
                "method": "session.status",
                "params": {
                    "status": {"type": "idle"},
                    "usage": {
                        "input_tokens": 11,
                        "output_tokens": 7,
                        "tool_calls": 1,
                    },
                    "output": "DONE",
                },
            }
        )
    )

    result = session.send("run this", system="strict scaffold", model="gpt-5.4-mini")

    assert result.output == "DONE"
    assert session.calls == [
        (
            "session/send",
            {
                "prompt": "run this",
                "system": "strict scaffold",
                "model": "gpt-5.4-mini",
            },
        )
    ]
