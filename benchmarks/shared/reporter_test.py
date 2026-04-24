from __future__ import annotations

import json

from shared.config import BenchConfig
from shared.reporter import Reporter
from shared.result import TaskResult


def test_reporter_json_dict_and_write_extra_fields(tmp_path) -> None:
    result = TaskResult(
        task_id="task",
        variant="v1",
        run_index=0,
        success=True,
        wall_clock_ms=10,
        input_tokens=2,
        output_tokens=3,
        turns=1,
        tool_calls=4,
    )
    reporter = Reporter("bench", [result], BenchConfig())

    payload = reporter.json_dict(extra_fields={"coordination_summary": {"v1": {"runs": 1}}})
    assert payload["results"][0]["tool_calls"] == 4
    assert payload["coordination_summary"]["v1"]["runs"] == 1

    reporter.write(tmp_path, extra_fields={"extra": True})
    written = json.loads((tmp_path / "report.json").read_text())
    assert written["extra"] is True
