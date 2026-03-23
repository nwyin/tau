"""Tests for the LRU cache."""

from src.cache import LRUCache


def test_get_miss():
    cache = LRUCache(max_size=2)
    assert cache.get("missing") is None


def test_put_and_get():
    cache = LRUCache(max_size=2)
    cache.put("a", 1)
    assert cache.get("a") == 1


def test_eviction():
    cache = LRUCache(max_size=2)
    cache.put("a", 1)
    cache.put("b", 2)
    cache.put("c", 3)  # should evict "a"
    assert cache.get("a") is None
    assert cache.get("b") == 2
    assert cache.get("c") == 3


def test_lru_order():
    cache = LRUCache(max_size=2)
    cache.put("a", 1)
    cache.put("b", 2)
    cache.get("a")  # "a" is now most recently used
    cache.put("c", 3)  # should evict "b", not "a"
    assert cache.get("a") == 1
    assert cache.get("b") is None
    assert cache.get("c") == 3


def test_stats():
    cache = LRUCache(max_size=2)
    cache.put("a", 1)
    cache.get("a")  # hit
    cache.get("b")  # miss
    stats = cache.stats()
    assert stats["hits"] == 1
    assert stats["misses"] == 1
    assert stats["hit_rate"] == 0.5
    assert stats["size"] == 1
    assert stats["max_size"] == 2
