from __future__ import annotations

import json

import pytest

from shared.store import ResultStore


def test_result_store_accepts_json_string_reports(tmp_path) -> None:
    store = ResultStore("bench")
    store.results_dir = tmp_path

    run_id = store.save(json.dumps({"benchmark": "bench", "run_id": "custom"}))

    assert run_id == "custom"
    assert json.loads((tmp_path / "custom.json").read_text())["benchmark"] == "bench"


def test_result_store_rejects_non_object_reports(tmp_path) -> None:
    store = ResultStore("bench")
    store.results_dir = tmp_path

    with pytest.raises(TypeError):
        store.save(["not", "a", "report"])  # type: ignore[arg-type]
