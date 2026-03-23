"""Schema validation module."""

from __future__ import annotations

import logging
from typing import Any

logger = logging.getLogger(__name__)

_REQUIRED_FIELDS = {"id", "type", "data"}


def validate_schema(data: dict[str, Any], schema: dict[str, Any] | None = None) -> bool:
    """Validate data against a schema, returning True if valid."""
    logger.info("Validating data with %d keys", len(data))
    required = set(schema.get("required", [])) if schema else _REQUIRED_FIELDS
    missing = required - set(data.keys())
    if missing:
        logger.warning("Missing required fields: %s", missing)
        return False
    for key, value in data.items():
        if value is None:
            logger.warning("Null value for field: %s", key)
            return False
    logger.info("Validation passed")
    return True
