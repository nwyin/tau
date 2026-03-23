"""Service health check module."""

from __future__ import annotations

import logging
import time

logger = logging.getLogger(__name__)

_service_registry: dict[str, float] = {}


def check_health(services: list[str] | None = None) -> dict[str, bool]:
    """Check health of registered services and return status map."""
    targets = services or list(_service_registry.keys())
    logger.info("Checking health of %d services", len(targets))
    results: dict[str, bool] = {}
    now = time.time()
    for svc in targets:
        last_seen = _service_registry.get(svc, 0.0)
        healthy = (now - last_seen) < 30.0 if last_seen > 0 else False
        results[svc] = healthy
        logger.debug("Service %s: %s", svc, "healthy" if healthy else "unhealthy")
    logger.info("Health check complete: %d/%d healthy", sum(results.values()), len(results))
    return results
