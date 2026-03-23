"""Webhook dispatching module."""

from __future__ import annotations

import hashlib
import logging
from typing import Any

logger = logging.getLogger(__name__)

_delivery_log: list[dict[str, Any]] = []


def dispatch_webhook(url: str, payload: dict[str, Any]) -> int:
    """Dispatch a webhook payload and return an HTTP-like status code."""
    logger.info("Dispatching webhook to %s", url)
    if not url.startswith(("http://", "https://")):
        logger.error("Invalid webhook URL: %s", url)
        return 400
    sig = hashlib.sha256(str(payload).encode()).hexdigest()[:12]
    delivery = {"url": url, "signature": sig, "status": 200}
    _delivery_log.append(delivery)
    logger.info("Webhook dispatched: %s (sig=%s)", url, sig)
    return 200
