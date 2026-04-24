from __future__ import annotations

import json

import shared.coordination_runner as runner
from shared.config import BenchConfig
from shared.coordination_runner import (
    CoordinationReportColumn,
    avg_score,
    build_coordination_summary,
    ratio_score,
    run_coordination_benchmark,
    write_coordination_reports,
)
from shared.result import TaskResult
from shared.variants import Variant


def _result(variant: str, success: bool, score: dict) -> TaskResult:
    return TaskResult(
        task_id="task",
        variant=variant,
        run_index=0,
        success=success,
        wall_clock_ms=10,
        input_tokens=1,
        output_tokens=2,
        turns=1,
        tool_calls=1,
        metadata={"session_success": True, "score": score},
    )


def test_build_coordination_summary_common_and_extra_metrics() -> None:
    summary = build_coordination_summary(
        [
            _result(
                "v1",
                True,
                {
                    "coordination_success": True,
                    "mechanism_success": True,
                    "timing_success": True,
                    "synthesis_success": False,
                    "episode_inject_has_both_sources": True,
                    "episode_inject_count_to_critic": 2,
                    "critic_doc_reads_after_required_writes": 1,
                    "top_level_tool_count": 3,
                },
            ),
            _result(
                "v1",
                False,
                {
                    "coordination_success": False,
                    "mechanism_success": False,
                    "timing_success": True,
                    "synthesis_success": True,
                    "episode_inject_has_both_sources": False,
                    "episode_inject_count_to_critic": 0,
                    "critic_doc_reads_after_required_writes": 2,
                    "top_level_tool_count": 1,
                },
            ),
        ],
        extra_metrics={
            "scaffold_rate": ratio_score("coordination_success"),
            "avg_top_tools": avg_score("top_level_tool_count"),
        },
        include_official_passes=True,
    )

    assert summary["v1"]["runs"] == 2
    assert summary["v1"]["official_passes"] == 1
    assert summary["v1"]["coordination_pass_rate"] == 0.5
    assert summary["v1"]["avg_critic_required_doc_reads_after_write"] == 1.5
    assert summary["v1"]["avg_top_tools"] == 2.0


def test_write_coordination_reports(tmp_path) -> None:
    summary = {"v1": {"runs": 1, "coordination_pass_rate": 1.0, "avg_reads": 2.5}}
    write_coordination_reports(
        tmp_path,
        "bench",
        BenchConfig(model="mock"),
        summary,
        [
            CoordinationReportColumn("Runs", "runs", "int"),
            CoordinationReportColumn("Coordination", "coordination_pass_rate"),
            CoordinationReportColumn("Reads", "avg_reads", "float2"),
        ],
    )

    assert json.loads((tmp_path / "coordination.json").read_text())["by_variant"]["v1"]["runs"] == 1
    markdown = (tmp_path / "coordination.md").read_text()
    assert "| v1 | 1 | 100.0% | 2.50 |" in markdown


def test_run_coordination_benchmark_writes_reports_and_stores_summary(tmp_path, monkeypatch, capsys) -> None:
    saved: dict[str, object] = {}

    class FakeResultStore:
        def __init__(self, benchmark: str) -> None:
            saved["benchmark"] = benchmark

        def save(self, report: dict) -> str:
            saved["report"] = report
            return "run-1"

    monkeypatch.setattr(runner, "ResultStore", FakeResultStore)

    def run_task(task: dict, variant: Variant, run_index: int, config: BenchConfig) -> TaskResult:
        return _result(
            variant.name,
            True,
            {
                "coordination_success": True,
                "mechanism_success": True,
                "timing_success": True,
                "synthesis_success": True,
                "episode_inject_has_both_sources": True,
                "episode_inject_count_to_critic": 1,
                "critic_doc_reads_after_required_writes": 2,
            },
        )

    results = run_coordination_benchmark(
        benchmark_name="bench",
        tasks=[{"id": "task"}],
        variants=[Variant(name="v1", description="")],
        config=BenchConfig(model="mock", runs_per_task=1, output_dir=tmp_path),
        json_output=False,
        run_task=run_task,
        status_line=lambda result: "  -> PASS",
        extra_metrics=None,
        report_columns=[CoordinationReportColumn("Runs", "runs", "int")],
    )

    assert len(results) == 1
    assert json.loads((tmp_path / "report.json").read_text())["summary"]["total"] == 1
    assert json.loads((tmp_path / "coordination.json").read_text())["by_variant"]["v1"]["runs"] == 1
    assert saved["benchmark"] == "bench"
    assert saved["report"]["coordination_summary"]["v1"]["runs"] == 1
    assert "Stored as run: run-1" in capsys.readouterr().err
