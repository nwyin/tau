"""Tests for the data processor."""

from src.cache import LRUCache
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


def test_process_with_cache():
    cache = LRUCache(max_size=10)
    results = process([1, 2, 1], cache=cache)
    assert len(results) == 3
    stats = cache.stats()
    assert stats["hits"] == 1  # second fetch of record 1 is a cache hit
    assert stats["misses"] == 2  # first fetch of 1 and 2 are misses
