Add an in-memory LRU cache layer to the data processing pipeline. Specifically:

1. Create `src/cache.py` with an `LRUCache` class that has `get(key)`, `put(key, value)`, and `stats()` methods. Max size should be configurable (default 128). Track hits and misses.
2. Update `src/processor.py` to use the cache: before calling `fetch_record`, check the cache. After fetching, store in cache.
3. Update `src/main.py` to create the cache and pass it to the processor.
4. Add `src/config.py` with a `Config` dataclass that has a `cache_size: int = 128` field. Update `main.py` to use it.
5. Write `tests/test_cache.py` with tests for cache get/put, eviction, and stats.