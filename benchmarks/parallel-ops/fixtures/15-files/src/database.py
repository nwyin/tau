"""Database connection module."""

from __future__ import annotations

import logging

logger = logging.getLogger(__name__)

_connections: dict[str, bool] = {}


def connect_db(host: str, port: int = 5432) -> bool:
    """Establish a database connection and return success status."""
    dsn = f"{host}:{port}"
    logger.info("Connecting to database at %s", dsn)
    if dsn in _connections:
        logger.info("Reusing existing connection to %s", dsn)
        return True
    # Simulate connection attempt
    success = len(host) > 0 and 1 <= port <= 65535
    if success:
        _connections[dsn] = True
        logger.info("Connected to %s", dsn)
    else:
        logger.error("Failed to connect to %s", dsn)
    return success
