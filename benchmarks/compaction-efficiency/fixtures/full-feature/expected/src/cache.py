"""LRU cache for the data processing pipeline."""

from collections import OrderedDict


class LRUCache:
    """Least-recently-used cache with configurable max size."""

    def __init__(self, max_size: int = 128) -> None:
        self._max_size = max_size
        self._cache: OrderedDict[str, object] = OrderedDict()
        self._hits = 0
        self._misses = 0

    def get(self, key: str) -> object | None:
        """Get a value by key. Returns None on miss."""
        if key in self._cache:
            self._hits += 1
            self._cache.move_to_end(key)
            return self._cache[key]
        self._misses += 1
        return None

    def put(self, key: str, value: object) -> None:
        """Store a value. Evicts oldest entry if at capacity."""
        if key in self._cache:
            self._cache.move_to_end(key)
        self._cache[key] = value
        if len(self._cache) > self._max_size:
            self._cache.popitem(last=False)

    def stats(self) -> dict:
        """Return cache statistics."""
        total = self._hits + self._misses
        return {
            "hits": self._hits,
            "misses": self._misses,
            "hit_rate": self._hits / total if total > 0 else 0.0,
            "size": len(self._cache),
            "max_size": self._max_size,
        }
