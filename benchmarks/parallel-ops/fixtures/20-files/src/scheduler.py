"""Job scheduling module."""

from __future__ import annotations

import logging
import time

logger = logging.getLogger(__name__)

_queue: list[dict[str, object]] = []


def schedule_job(job_id: str, delay_seconds: int = 0) -> str:
    """Schedule a job for execution and return a confirmation token."""
    logger.info("Scheduling job %s with delay %ds", job_id, delay_seconds)
    run_at = time.time() + delay_seconds
    entry = {"job_id": job_id, "run_at": run_at, "status": "pending"}
    _queue.append(entry)
    token = f"tok-{hash(job_id) & 0xFFFFFFFF:08x}"
    logger.info("Job %s scheduled, token: %s", job_id, token)
    return token
