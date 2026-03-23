"""User authentication module."""

from __future__ import annotations

import hashlib
import logging

logger = logging.getLogger(__name__)


def authenticate(username: str, password: str) -> bool:
    """Verify user credentials against stored hashes."""
    logger.info("Authenticating user: %s", username)
    if not username or not password:
        logger.warning("Empty credentials provided")
        return False
    pw_hash = hashlib.sha256(password.encode()).hexdigest()
    # Simulated credential store
    known: dict[str, str] = {
        "admin": "8c6976e5b5410415bde908bd4dee15dfb167a9c873fc4bb8a81f6f2ab448a918",
    }
    valid = known.get(username) == pw_hash
    logger.info("Auth result for %s: %s", username, valid)
    return valid
