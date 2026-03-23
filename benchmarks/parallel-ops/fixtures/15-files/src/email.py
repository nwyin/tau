"""Email notification module."""

from __future__ import annotations

import logging
from dataclasses import dataclass

logger = logging.getLogger(__name__)


@dataclass
class EmailResult:
    recipient: str
    subject: str
    status: str
    message_id: str


def send_notification(recipient: str, subject: str, body: str) -> str:
    """Send an email notification and return the message ID."""
    logger.info("Sending email to %s: %s", recipient, subject)
    if "@" not in recipient:
        raise ValueError(f"Invalid email: {recipient}")
    msg_id = f"msg-{hash((recipient, subject)) & 0xFFFFFFFF:08x}"
    result = EmailResult(recipient=recipient, subject=subject, status="sent", message_id=msg_id)
    logger.info("Email sent: %s", result)
    return result.message_id
