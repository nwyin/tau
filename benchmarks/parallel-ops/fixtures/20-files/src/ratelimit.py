"""Rate limiting module."""

from __future__ import annotations

import logging
import time

logger = logging.getLogger(__name__)

_windows: dict[str, list[float]] = {}
_MAX_REQUESTS = 100
_WINDOW_SECONDS = 60.0


def check_limit(client_id: str, action: str) -> bool:
    """Check if a client has exceeded the rate limit for an action."""
    key = f"{client_id}:{action}"
    logger.info("Checking rate limit for %s", key)
    now = time.time()
    window = _windows.setdefault(key, [])
    window[:] = [ts for ts in window if now - ts < _WINDOW_SECONDS]
    if len(window) >= _MAX_REQUESTS:
        logger.warning("Rate limit exceeded for %s (%d requests)", key, len(window))
        return False
    window.append(now)
    logger.debug("Rate limit ok for %s: %d/%d", key, len(window), _MAX_REQUESTS)
    return True
