"""Report generation module."""

from __future__ import annotations

import json
import logging
from typing import Any

logger = logging.getLogger(__name__)


def generate_report(data: dict[str, Any], fmt: str = "json") -> str:
    """Generate a formatted report from data."""
    logger.info("Generating %s report from %d fields", fmt, len(data))
    if fmt == "json":
        output = json.dumps(data, indent=2, default=str)
    elif fmt == "text":
        lines = [f"{k}: {v}" for k, v in sorted(data.items())]
        output = "\n".join(lines)
    else:
        raise ValueError(f"Unsupported format: {fmt}")
    logger.info("Report generated: %d chars", len(output))
    return output
