"""Tests for the data processor."""

from src.processor import fetch_record, process, transform


def test_fetch_record():
    record = fetch_record(1)
    assert record["id"] == 1
    assert record["value"] == 10


def test_transform():
    result = transform({"id": 1, "value": 10, "status": "active"})
    assert result["computed"] == 21
    assert result["label"] == "item-1"


def test_process():
    results = process([1, 2])
    assert len(results) == 2
    assert results[0]["id"] == 1
    assert results[1]["id"] == 2
