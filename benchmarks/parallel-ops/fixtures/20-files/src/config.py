"""Configuration loading module."""

from __future__ import annotations

import json
import logging
from pathlib import Path
from typing import Any

logger = logging.getLogger(__name__)

_DEFAULTS: dict[str, Any] = {
    "host": "localhost",
    "port": 8080,
    "debug": False,
    "log_level": "INFO",
}


def load_config(path: str | None = None) -> dict[str, Any]:
    """Load configuration from a JSON file, falling back to defaults."""
    logger.info("Loading config from: %s", path or "<defaults>")
    config = dict(_DEFAULTS)
    if path is not None:
        config_path = Path(path)
        if config_path.exists():
            with open(config_path) as f:
                overrides = json.load(f)
            config.update(overrides)
            logger.info("Applied %d overrides from %s", len(overrides), path)
    return config
