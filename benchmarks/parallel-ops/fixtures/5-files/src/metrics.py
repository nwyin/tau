"""Event tracking and metrics module."""

from __future__ import annotations

import json
import logging
import time
from typing import Any

logger = logging.getLogger(__name__)

_events: list[dict[str, Any]] = []


def track_event(event_name: str, properties: dict[str, Any] | None = None) -> str:
    """Record a tracking event and return its JSON representation."""
    logger.info("Tracking event: %s", event_name)
    event: dict[str, Any] = {
        "name": event_name,
        "timestamp": time.time(),
        "properties": properties or {},
    }
    _events.append(event)
    serialized = json.dumps(event)
    logger.debug("Event recorded: %s", serialized)
    return serialized
