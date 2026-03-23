"""Payment processing module."""

from __future__ import annotations

import logging
from typing import Any

logger = logging.getLogger(__name__)

SUPPORTED_CURRENCIES = {"USD", "EUR", "GBP", "JPY"}


def process_payment(amount: float, currency: str = "USD") -> dict[str, Any]:
    """Process a payment and return a transaction record."""
    logger.info("Processing payment: %.2f %s", amount, currency)
    if currency not in SUPPORTED_CURRENCIES:
        raise ValueError(f"Unsupported currency: {currency}")
    if amount <= 0:
        raise ValueError(f"Invalid amount: {amount}")
    fee = amount * 0.029 + 0.30
    net = amount - fee
    record: dict[str, Any] = {
        "amount": amount,
        "currency": currency,
        "fee": round(fee, 2),
        "net": round(net, 2),
        "status": "completed",
    }
    logger.info("Payment processed: %s", record)
    return record
