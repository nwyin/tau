"""Cache invalidation module."""

from __future__ import annotations

import logging
import time

logger = logging.getLogger(__name__)

_store: dict[str, tuple[float, str]] = {}


def invalidate_cache(keys: list[str]) -> int:
    """Remove entries from the cache, returning the count of keys removed."""
    logger.info("Invalidating %d cache keys", len(keys))
    removed = 0
    for key in keys:
        if key in _store:
            del _store[key]
            removed += 1
            logger.debug("Removed key: %s", key)
    logger.info("Cache invalidation complete: %d/%d removed", removed, len(keys))
    return removed


def _set(key: str, value: str, ttl: float = 60.0) -> None:
    _store[key] = (time.monotonic() + ttl, value)
