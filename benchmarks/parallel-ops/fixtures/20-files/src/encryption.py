"""Payload encryption module."""

from __future__ import annotations

import hashlib
import logging

logger = logging.getLogger(__name__)


def encrypt_payload(data: bytes, key: str) -> bytes:
    """Encrypt data using a simple XOR cipher with a derived key stream."""
    logger.info("Encrypting %d bytes", len(data))
    key_bytes = hashlib.sha256(key.encode()).digest()
    result = bytearray(len(data))
    for i, byte in enumerate(data):
        result[i] = byte ^ key_bytes[i % len(key_bytes)]
    logger.info("Encryption complete: %d bytes", len(result))
    return bytes(result)
