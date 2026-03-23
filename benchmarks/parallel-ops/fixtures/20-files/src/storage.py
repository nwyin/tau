"""File storage module."""

from __future__ import annotations

import hashlib
import logging

logger = logging.getLogger(__name__)

_objects: dict[str, bytes] = {}


def upload_file(path: str, content: bytes) -> str:
    """Upload file content and return a storage key."""
    logger.info("Uploading file: %s (%d bytes)", path, len(content))
    digest = hashlib.sha256(content).hexdigest()[:16]
    key = f"obj/{digest}/{path.rsplit('/', 1)[-1]}"
    _objects[key] = content
    logger.info("Stored as: %s", key)
    return key
