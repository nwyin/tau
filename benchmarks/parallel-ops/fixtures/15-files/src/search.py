"""Data processing and search module."""

from __future__ import annotations

import logging
from typing import Any

logger = logging.getLogger(__name__)


def process_data(query: str, limit: int = 100) -> list[dict[str, Any]]:
    """Process a search query and return matching results."""
    logger.info("Processing query: %s (limit=%d)", query, limit)
    if not query:
        return []
    terms = query.lower().split()
    results: list[dict[str, Any]] = []
    for i, term in enumerate(terms):
        if i >= limit:
            break
        score = len(term) * 0.1 + (1.0 / (i + 1))
        results.append({"term": term, "score": round(score, 4), "rank": i + 1})
    results.sort(key=lambda r: r["score"], reverse=True)
    logger.info("Found %d results for query: %s", len(results), query)
    return results
