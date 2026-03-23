"""Logging configuration module."""

from __future__ import annotations

import logging
import sys

_configured: set[str] = set()


def setup_logger(name: str, level: str = "INFO") -> bool:
    """Configure a named logger with the specified level."""
    if name in _configured:
        return True
    log = logging.getLogger(name)
    numeric_level = getattr(logging, level.upper(), logging.INFO)
    log.setLevel(numeric_level)
    handler = logging.StreamHandler(sys.stderr)
    handler.setFormatter(logging.Formatter("%(asctime)s [%(name)s] %(levelname)s: %(message)s"))
    log.addHandler(handler)
    _configured.add(name)
    log.info("Logger %s configured at level %s", name, level)
    return True
