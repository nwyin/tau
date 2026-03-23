"""Data transformation module."""

from __future__ import annotations

import logging
from typing import Any

logger = logging.getLogger(__name__)


def apply_transform(data: list[dict[str, Any]], mapping: dict[str, str] | None = None) -> list[dict[str, Any]]:
    """Apply field-name transformations to a list of records."""
    logger.info("Transforming %d records", len(data))
    if not mapping:
        return list(data)
    results: list[dict[str, Any]] = []
    for record in data:
        transformed: dict[str, Any] = {}
        for key, value in record.items():
            new_key = mapping.get(key, key)
            transformed[new_key] = value
        results.append(transformed)
    logger.info("Transformation complete: %d records", len(results))
    return results
